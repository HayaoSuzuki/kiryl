// Copyright (c) 2026 kiryl contributors
//! Weaker process-group cleanup, explicitly without containment or hard memory caps.
use std::io;

pub struct Group {
    pid: u32,
    pub reason: String,
}
impl Group {
    pub fn new(pid: u32, reason: String) -> Self {
        Self { pid, reason }
    }
    #[cfg_attr(not(unix), allow(clippy::unnecessary_wraps))] // common backend fallible interface
    pub fn terminate(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let pid = i32::try_from(self.pid).map_err(io::Error::other)?;
            // SAFETY: worker is unreaped and owns this process group. The guard
            // executes once, before wait; no reused PID can identify another tree.
            if unsafe { libc::kill(-pid, libc::SIGKILL) } != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = self.pid;
        }
        Ok(())
    }
}
