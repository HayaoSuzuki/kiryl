// Copyright (c) 2026 kiryl contributors
//! Aggregate job memory and completion-port evidence; assignment precedes target import.
use std::{
    ffi::c_void,
    io,
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    process::Child,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT},
    System::{
        IO::{CreateIoCompletionPort, GetQueuedCompletionStatus},
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectAssociateCompletionPortInformation, JobObjectBasicAccountingInformation,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
            TerminateJobObject,
        },
        SystemServices::JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
    },
};

struct Handle(HANDLE);
impl Handle {
    fn new(raw: HANDLE) -> io::Result<Self> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: handle is owned, live, and closed exactly once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct Job {
    handle: Handle,
    port: Handle,
    oom: bool,
}
fn set<T>(job: HANDLE, class: i32, info: &T) -> io::Result<()> {
    let size = const {
        assert!(size_of::<T>() <= u32::MAX as usize);
        // The constant assertion rejects an ABI size that cannot fit DWORD.
        #[allow(clippy::cast_possible_truncation)]
        {
            size_of::<T>() as u32
        }
    };
    // SAFETY: callers pair the exact information class with its initialized
    // Windows ABI structure. The borrowed structure lives throughout this call.
    if unsafe {
        SetInformationJobObject(job, class, std::ptr::from_ref(info).cast::<c_void>(), size)
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
// Bind this job's notification stream before assigning its first process.
fn associate_port(job: &Handle, port: &Handle) -> io::Result<()> {
    let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        CompletionKey: std::ptr::dangling_mut::<c_void>(),
        CompletionPort: port.0,
    };
    set(
        job.0,
        JobObjectAssociateCompletionPortInformation,
        &association,
    )
}

impl Job {
    pub fn attach(child: &Child, memory: u64) -> io::Result<Self> {
        // SAFETY: null pointers request an unnamed job with default security.
        let job = Handle::new(unsafe { CreateJobObjectW(null(), null()) })?;
        // SAFETY: documented zero-initialized ABI structure has no references.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.JobMemoryLimit = usize::try_from(memory).map_err(io::Error::other)?;
        if info.JobMemoryLimit == 0 {
            return Err(io::Error::other("zero job memory limit"));
        }
        set(job.0, JobObjectExtendedLimitInformation, &info)?;
        // SAFETY: INVALID_HANDLE_VALUE creates a standalone completion port.
        let port =
            Handle::new(unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 1) })?;
        associate_port(&job, &port)?;
        // SAFETY: job and process are live owned handles; bootstrap cannot import
        // target code until the parent acknowledges this successful assignment.
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: job,
            port,
            oom: false,
        })
    }
    pub fn oom_observed(&mut self) -> io::Result<bool> {
        loop {
            let mut message = 0;
            let mut key = 0;
            let mut process = null_mut();
            // SAFETY: pointers reference live local outputs; process is a numeric
            // notification payload and is never dereferenced as OVERLAPPED.
            let result = unsafe {
                GetQueuedCompletionStatus(
                    self.port.0,
                    &raw mut message,
                    &raw mut key,
                    &raw mut process,
                    0,
                )
            };
            if result == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error().and_then(|e| u32::try_from(e).ok()) == Some(WAIT_TIMEOUT) {
                    return Ok(self.oom);
                }
                return Err(error);
            }
            if message == JOB_OBJECT_MSG_JOB_MEMORY_LIMIT {
                self.oom = true;
            }
        }
    }
    fn active(&self) -> io::Result<u32> {
        // SAFETY: documented plain ABI structure supports zero initialization.
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let size = const {
            assert!(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() <= u32::MAX as usize);
            // The constant assertion guarantees this Windows ABI size fits DWORD.
            #[allow(clippy::cast_possible_truncation)]
            {
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32
            }
        };
        // SAFETY: information class/layout/size match; output storage is live.
        if unsafe {
            QueryInformationJobObject(
                self.handle.0,
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast::<c_void>(),
                size,
                null_mut(),
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(info.ActiveProcesses)
        }
    }
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: the job handle is owned and includes only the assigned tree.
        if unsafe { TerminateJobObject(self.handle.0, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.wait_quiescence(Instant::now() + Duration::from_millis(500))
    }

    fn wait_quiescence(&self, deadline: Instant) -> io::Result<()> {
        while self.active()? != 0 {
            if Instant::now() > deadline {
                return Err(io::Error::other("job cleanup did not quiesce"));
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::process::{Command, Stdio};
    use windows_sys::Win32::System::{
        IO::PostQueuedCompletionStatus,
        SystemServices::{
            JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO, JOB_OBJECT_MSG_NEW_PROCESS,
            JOB_OBJECT_MSG_NOTIFICATION_LIMIT, JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT,
        },
    };

    fn owned_job_handle() -> Handle {
        // SAFETY: default security and null name create one fresh owned job.
        Handle::new(unsafe { CreateJobObjectW(null(), null()) }).unwrap()
    }
    fn owned_port() -> Handle {
        // SAFETY: the sentinel requests a new standalone completion port.
        Handle::new(unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 1) })
            .unwrap()
    }
    fn empty_job() -> Job {
        Job {
            handle: owned_job_handle(),
            port: owned_port(),
            oom: false,
        }
    }
    fn post(port: &Handle, message: u32) {
        // SAFETY: port is owned and live; a null OVERLAPPED is an opaque payload.
        assert_ne!(
            unsafe { PostQueuedCompletionStatus(port.0, message, 1, null()) },
            0
        );
    }
    struct OwnedChild(Child);
    impl OwnedChild {
        fn sleeping() -> Self {
            let root = std::env::var_os("SystemRoot").unwrap();
            Self(
                Command::new(
                    std::path::PathBuf::from(root)
                        .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
                )
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Start-Sleep -Seconds 60",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
            )
        }
        fn new() -> Self {
            let root = std::env::var_os("SystemRoot").unwrap();
            Self(
                Command::new(std::path::PathBuf::from(root).join("System32/cmd.exe"))
                    .args(["/D", "/C", "set /p kiryl_owned_test="])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            )
        }
    }
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[rstest::rstest]
    #[case(null_mut())]
    #[case(INVALID_HANDLE_VALUE)]
    fn invalid_api_handle_is_rejected(#[case] handle: HANDLE) {
        assert!(Handle::new(handle).is_err());
    }
    #[rstest::rstest]
    #[case(JOB_OBJECT_MSG_JOB_MEMORY_LIMIT, true)]
    #[case(JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT, false)]
    #[case(JOB_OBJECT_MSG_NEW_PROCESS, false)]
    #[case(JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO, false)]
    #[case(JOB_OBJECT_MSG_NOTIFICATION_LIMIT, false)]
    #[case(0, false)]
    fn completion_evidence_classifies_only_job_memory_limit(
        #[case] message: u32,
        #[case] expected: bool,
    ) {
        let mut job = empty_job();
        post(&job.port, message);
        assert_eq!(job.oom_observed().unwrap(), expected);
        assert_eq!(job.oom_observed().unwrap(), expected);
    }
    proptest! {
        #[test]
        fn arbitrary_completion_payload_only_matches_job_memory_event(message in any::<u32>()) {
            let mut job = empty_job();
            post(&job.port, message);
            prop_assert_eq!(job.oom_observed().unwrap(), message == JOB_OBJECT_MSG_JOB_MEMORY_LIMIT);
        }
    }
    #[test]
    fn drained_completion_port_preserves_prior_memory_evidence() {
        let mut job = empty_job();
        post(&job.port, JOB_OBJECT_MSG_JOB_MEMORY_LIMIT);
        post(&job.port, JOB_OBJECT_MSG_NOTIFICATION_LIMIT);
        assert!(job.oom_observed().unwrap());
        assert!(job.oom_observed().unwrap());
    }
    #[test]
    fn empty_completion_port_does_not_claim_memory_evidence() {
        assert!(!empty_job().oom_observed().unwrap());
    }
    #[test]
    fn completion_query_failure_is_not_oom_evidence() {
        let mut job = Job {
            handle: owned_job_handle(),
            port: owned_job_handle(),
            oom: true,
        };
        assert!(job.oom_observed().is_err());
    }
    #[test]
    fn accounting_query_rejects_wrong_owned_object_type() {
        let job = Job {
            handle: owned_port(),
            port: owned_port(),
            oom: false,
        };
        assert!(job.active().is_err());
    }
    #[test]
    fn completion_port_association_accepts_owned_objects() {
        let job = owned_job_handle();
        let port = owned_port();
        associate_port(&job, &port).unwrap();
    }
    #[test]
    fn completion_port_association_rejects_non_job_owned_object() {
        let wrong_job = owned_port();
        let port = owned_port();
        assert!(associate_port(&wrong_job, &port).is_err());
    }
    #[test]
    fn limit_configuration_rejects_wrong_owned_object_type() {
        let port = owned_port();
        // SAFETY: the ABI information structure explicitly supports zero initialization.
        let info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        assert!(set(port.0, JobObjectExtendedLimitInformation, &info).is_err());
    }
    #[test]
    fn termination_rejects_wrong_owned_object_type() {
        let job = Job {
            handle: owned_port(),
            port: owned_port(),
            oom: false,
        };
        assert!(job.terminate().is_err());
    }
    #[test]
    fn occupied_job_reports_expired_cleanup_deadline() {
        let mut child = OwnedChild::sleeping();
        let job = Job::attach(&child.0, 1 << 30).unwrap();
        let deadline = Instant::now() + Duration::from_millis(500);
        assert_eq!(
            job.wait_quiescence(deadline)
                .map_err(|error| error.to_string()),
            Err("job cleanup did not quiesce".to_owned()),
            "owned sleeper state: {:?}",
            child.0.try_wait()
        );
    }
    #[test]
    fn empty_job_has_zero_active_processes() {
        assert_eq!(empty_job().active().unwrap(), 0);
    }
    #[test]
    fn empty_job_terminates_idempotently() {
        let job = empty_job();
        job.terminate().unwrap();
        job.terminate().unwrap();
        assert_eq!(job.active().unwrap(), 0);
    }
    #[test]
    fn zero_memory_limit_fails_before_assigning_owned_child() {
        let child = OwnedChild::new();
        assert_eq!(
            Job::attach(&child.0, 0).err().unwrap().to_string(),
            "zero job memory limit"
        );
    }
    #[test]
    fn exited_owned_child_cannot_be_assigned() {
        let mut child = OwnedChild::new();
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        assert!(Job::attach(&child.0, 1 << 30).is_err());
    }
    #[test]
    fn attached_owned_child_is_queried_and_terminated() {
        let mut child = OwnedChild::new();
        let job = Job::attach(&child.0, 1 << 30).unwrap();
        assert_eq!(job.active().unwrap(), 1);
        // SAFETY: the plain ABI output is zero-initialized and has the exact queried size.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        // SAFETY: the owned job is live and information class, buffer and DWORD size agree.
        assert_ne!(
            unsafe {
                QueryInformationJobObject(
                    job.handle.0,
                    JobObjectExtendedLimitInformation,
                    (&raw mut info).cast::<c_void>(),
                    u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap(),
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(
            info.BasicLimitInformation.LimitFlags
                & (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_JOB_MEMORY),
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_JOB_MEMORY
        );
        assert_eq!(info.JobMemoryLimit, 1 << 30);
        job.terminate().unwrap();
        assert_eq!(job.active().unwrap(), 0);
        assert!(!child.0.wait().unwrap().success());
    }
}
