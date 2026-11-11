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
        if let Some(mut anchor) = self.anchor.take() {
            let _ = anchor.kill();
            anchor.wait()?;
        }
        Ok(())
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
