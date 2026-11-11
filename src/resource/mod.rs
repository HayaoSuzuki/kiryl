// Copyright (c) 2026 kiryl contributors
//! Experiment-only native containment, with truthful portable fallback.
//!
//! Mechanisms informed by hoimin resource/{linux,windows,portable}.rs at
//! 542ec59c002d72fdff0137aa90113ffa59d70844 (Elastic-2.0). This smaller synchronous
//! probe has no run-wide process framework or product compatibility guarantee.
use crate::executor::Limits;
use serde::Serialize;
use std::{
    io,
    process::{Child, Command, Stdio},
};

#[cfg(target_os = "linux")]
mod linux;
mod portable;
#[cfg(windows)]
mod windows;

pub type ResourceError = io::Error;

#[derive(Clone, Debug, Serialize)]
pub struct Capabilities {
    pub tree_memory_hard: bool,
    pub tree_cleanup: bool,
    pub backend: String,
    pub oom_evidence_complete: bool,
    pub reason: Option<String>,
}
impl Capabilities {
    fn portable(reason: String) -> Self {
        Self {
            tree_memory_hard: false,
            tree_cleanup: false,
            backend: "portable".into(),
            oom_evidence_complete: false,
            reason: Some(reason),
        }
    }
    #[cfg(any(target_os = "linux", windows))]
    fn hard(backend: &str) -> Self {
        Self {
            tree_memory_hard: true,
            tree_cleanup: true,
            backend: backend.into(),
            oom_evidence_complete: cfg!(target_os = "linux"),
            reason: None,
        }
    }
}

enum Backend {
    #[cfg(target_os = "linux")]
    Linux(linux::Group),
    #[cfg(windows)]
    Windows(windows::Job),
    Portable(portable::Group),
}

pub struct Guard {
    backend: Backend,
    terminated: bool,
}
impl Guard {
    /// Terminate the owned tree, once; never reuse numeric process identities.
    /// # Errors
    /// Reports containment or cleanup failures rather than claiming success.
    pub fn terminate_tree(&mut self) -> Result<(), ResourceError> {
        if self.terminated {
            return Ok(());
        }
        // Consume numeric cleanup authority before attempting a system call.
        // A failed attempt is reported, never retried after root PID reuse.
        self.terminated = true;
        match &mut self.backend {
            #[cfg(target_os = "linux")]
            Backend::Linux(group) => group.terminate()?,
            #[cfg(windows)]
            Backend::Windows(job) => job.terminate()?,
            Backend::Portable(group) => group.terminate()?,
        }
        Ok(())
    }
    /// Read backend evidence; a signal or arbitrary exit code is never OOM proof.
    /// # Errors
    /// Reports unavailable kernel counters/notifications.
    #[allow(clippy::unnecessary_wraps)] // Native Linux/Windows backends can fail.
    pub fn oom_observed(&mut self) -> Result<bool, ResourceError> {
        match &mut self.backend {
            #[cfg(target_os = "linux")]
            Backend::Linux(group) => group.oom_observed(),
            #[cfg(windows)]
            Backend::Windows(job) => job.oom_observed(),
            Backend::Portable(_) => Ok(false),
        }
    }
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        match &self.backend {
            #[cfg(target_os = "linux")]
            Backend::Linux(_) => Capabilities::hard("cgroup-v2"),
            #[cfg(windows)]
            Backend::Windows(_) => Capabilities::hard("job-object"),
            Backend::Portable(group) => Capabilities::portable(group.reason.clone()),
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.terminate_tree();
    }
}

fn native_attach(worker: &mut Child, limits: &Limits) -> io::Result<Guard> {
    #[cfg(target_os = "linux")]
    let backend = Backend::Linux(linux::Group::attach(worker, limits.tree_memory_bytes)?);
    #[cfg(windows)]
    let backend = Backend::Windows(windows::Job::attach(worker, limits.tree_memory_bytes)?);
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (worker, limits);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no native hard memory backend on this platform",
        ))
    }
    #[cfg(any(target_os = "linux", windows))]
    Ok(Guard {
        backend,
        terminated: false,
    })
}

/// Probe actual creation, limit configuration, assignment, and cleanup permission.
#[must_use]
pub fn detect() -> Capabilities {
    match detect_native() {
        Ok(caps) => caps,
        Err(error) => Capabilities::portable(error.to_string()),
    }
}
fn detect_native() -> io::Result<Capabilities> {
    #[cfg(unix)]
    let mut command = {
        let mut c = Command::new("/bin/sleep");
        c.arg("30");
        c
    };
    #[cfg(windows)]
    let mut command = {
        let root =
            std::env::var_os("SystemRoot").ok_or_else(|| io::Error::other("missing SystemRoot"))?;
        let mut c = Command::new(std::path::PathBuf::from(root).join("System32/cmd.exe"));
        c.args(["/D", "/C", "set /p kiryl_capability_probe="]);
        c
    };
    #[cfg(not(any(unix, windows)))]
    return Err(io::Error::other("no capability probe for platform"));
    #[cfg(any(unix, windows))]
    {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let result = native_attach(&mut child, &Limits::default());
        let result = result.and_then(|mut guard| {
            let caps = guard.capabilities();
            guard.terminate_tree()?;
            Ok(caps)
        });
        let _ = child.kill();
        let _ = child.wait();
        result
    }
}

/// Attach a worker before authorizing target imports, or explicitly disclose fallback.
/// # Errors
/// Refuses unsupported hard caps unless best effort was explicitly selected.
pub fn attach(
    worker: &mut Child,
    limits: &Limits,
    allow_best_effort: bool,
) -> Result<Guard, ResourceError> {
    match native_attach(worker, limits) {
        Ok(guard) => Ok(guard),
        Err(error) if allow_best_effort => Ok(Guard {
            backend: Backend::Portable(portable::Group::attach(worker.id(), error.to_string())?),
            terminated: false,
        }),
        Err(error) => Err(error),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn cleanup_correspondence_parameterization_covers_corpus() {
        assert_eq!(
            include_str!("../../formal/resource-cases.tsv")
                .lines()
                .count(),
            9
        );
    }
    #[rstest::rstest]
    fn cleanup_failure_consumes_authority_once_lean_correspondence(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8)] row: usize,
    ) {
        // internal-fixture: invalid PID fails conversion before any OS signal.
        // The observed Err counts attempts, independently of the guard state bit.
        let line = include_str!("../../formal/resource-cases.tsv")
            .lines()
            .nth(row)
            .unwrap();
        let fields: Vec<_> = line.split('\t').collect();
        let requests: usize = fields[0].parse().unwrap();
        let mut guard = Guard {
            backend: Backend::Portable(portable::Group::new(
                u32::MAX,
                "injected invalid PID".into(),
            )),
            terminated: false,
        };
        let attempts = (0..requests)
            .filter(|_| guard.terminate_tree().is_err())
            .count();
        assert_eq!(guard.terminated, fields[1] == "true");
        assert_eq!(attempts, fields[2].parse::<usize>().unwrap());
    }
}
