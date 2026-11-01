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
    let size = u32::try_from(size_of::<T>()).map_err(io::Error::other)?;
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
        let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
            CompletionKey: std::ptr::dangling_mut::<c_void>(),
            CompletionPort: port.0,
        };
        set(
            job.0,
            JobObjectAssociateCompletionPortInformation,
            &association,
        )?;
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
        let size = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .map_err(io::Error::other)?;
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
        let start = Instant::now();
        while self.active()? != 0 {
            if start.elapsed() > Duration::from_millis(500) {
                return Err(io::Error::other("job cleanup did not quiesce"));
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
}
