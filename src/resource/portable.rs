// Copyright (c) 2026 kiryl contributors
//! Weaker process-group cleanup, explicitly without containment or hard memory caps.
use std::{io, process::Child};

#[cfg(unix)]
use std::process::{Command, Stdio};

pub struct Group {
    pid: u32,
    pub reason: String,
    anchor: Option<Child>,
}
impl Group {
    #[cfg(all(test, unix))]
    pub fn new(pid: u32, reason: String) -> Self {
        Self {
            pid,
            reason,
            anchor: None,
        }
    }

    #[cfg_attr(not(unix), allow(clippy::unnecessary_wraps))] // common fallible backend creation
    pub fn attach(pid: u32, reason: String) -> io::Result<Self> {
        #[cfg(unix)]
        let anchor = {
            use std::os::unix::process::CommandExt;
            let group = i32::try_from(pid).map_err(io::Error::other)?;
            // Keep an owned live member until cleanup. On macOS a group containing
            // only an unreaped zombie can reject killpg with EPERM. Holding stdin
            // open makes this trusted system utility block without a time limit.
            Some(
                Command::new("/bin/cat")
                    .process_group(group)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?,
            )
        };
        #[cfg(not(unix))]
        let anchor = None;
        Ok(Self {
            pid,
            reason,
            anchor,
        })
    }
    pub fn terminate(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let pid = i32::try_from(self.pid).map_err(io::Error::other)?;
            // SAFETY: worker owns this group and remains unreaped until cleanup;
            // the live anchor preserves signaling authority even after worker exit.
            let failed = unsafe { libc::kill(-pid, libc::SIGKILL) } != 0;
            signal_result(failed.then(io::Error::last_os_error))?;
        }
        #[cfg(not(unix))]
        {
            let _ = self.pid;
        }
        if let Some(mut anchor) = self.anchor.take() {
            let _ = anchor.kill();
            return anchor.wait().map(|_| ());
        }
        Ok(())
    }
}
// A vanished owned group needs no further signal. Other OS errors must remain
// visible so the supervisor cannot mistake failed cleanup for successful teardown.
#[cfg(unix)]
fn signal_result(error: Option<io::Error>) -> io::Result<()> {
    match error {
        Some(error) if error.raw_os_error() != Some(libc::ESRCH) => Err(error),
        _ => Ok(()),
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        // Never retry a numeric group signal; owned child handles remain safe.
        if let Some(mut anchor) = self.anchor.take() {
            let _ = anchor.kill();
            let _ = anchor.wait();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    struct OwnedWorker(Child);
    impl Drop for OwnedWorker {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn worker() -> OwnedWorker {
        OwnedWorker(
            Command::new("/bin/cat")
                .process_group(0)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    #[test]
    fn attaching_an_unrepresentable_group_id_fails_before_starting_an_anchor() {
        assert!(Group::attach(u32::MAX, "test".into()).is_err());
    }

    #[test]
    fn terminating_an_unrepresentable_group_id_does_not_signal() {
        assert!(Group::new(u32::MAX, "test".into()).terminate().is_err());
    }

    #[test]
    fn dropping_a_live_group_reaps_its_owned_anchor() {
        let worker = worker();
        let group = Group::attach(worker.0.id(), "test".into()).unwrap();
        let anchor = i32::try_from(group.anchor.as_ref().unwrap().id()).unwrap();
        drop(group);
        let mut status = 0;
        // SAFETY: this is only a nonblocking wait for the anchor this test owned;
        // the normal Drop must already have reaped it, yielding ECHILD.
        assert_eq!(
            unsafe { libc::waitpid(anchor, &raw mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
    #[rstest::rstest]
    #[case::no_error(None, true)]
    #[case::already_gone(Some(libc::ESRCH), true)]
    #[case::permission_failure(Some(libc::EPERM), false)]
    fn group_signal_policy_preserves_unexpected_errors(
        #[case] error: Option<i32>,
        #[case] success: bool,
    ) {
        assert_eq!(
            signal_result(error.map(io::Error::from_raw_os_error)).is_ok(),
            success
        );
    }
    #[test]
    fn an_owned_live_group_can_be_cleaned_without_an_anchor() {
        let worker = worker();
        assert!(Group::new(worker.0.id(), "test".into()).terminate().is_ok());
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::Group;
    #[test]
    fn unsupported_group_cleanup_defers_to_the_owned_process_handle() {
        let mut group = Group::attach(0, "no group signaling backend".into()).unwrap();
        assert!(group.terminate().is_ok());
    }
}
