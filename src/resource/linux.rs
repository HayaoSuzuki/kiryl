// Copyright (c) 2026 kiryl contributors
//! One owned cgroup per worker; unavailable delegation yields explicit fallback.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::ffi::OsStringExt,
    path::{Component, Path, PathBuf},
    process::Child,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
pub struct Group {
    path: PathBuf,
}

fn write_existing(path: &Path, value: &str) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .open(path)?
        .write_all(value.as_bytes())
}
fn unescape(field: &str) -> io::Result<PathBuf> {
    let mut bytes = Vec::new();
    let mut input = field.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'\\' {
            let mut number = 0u8;
            for _ in 0..3 {
                let digit = input
                    .next()
                    .filter(|b| (b'0'..=b'7').contains(b))
                    .ok_or_else(|| io::Error::other("bad mount escape"))?
                    - b'0';
                number = number
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(digit))
                    .ok_or_else(|| io::Error::other("bad mount escape"))?;
            }
            bytes.push(number);
        } else {
            bytes.push(byte);
        }
    }
    Ok(std::ffi::OsString::from_vec(bytes).into())
}
fn validate_parent(parent: &Path, mount: &Path, current: &Path) -> io::Result<()> {
    if !parent.is_absolute() || !parent.starts_with(mount) || !current.starts_with(parent) {
        return Err(io::Error::other(
            "KIRYL_CGROUP_PARENT must be an ancestor of this process within its cgroup-v2 mount",
        ));
    }
    Ok(())
}

fn parents() -> io::Result<Vec<PathBuf>> {
    // Explicit delegation changes only where owned worker groups are created.
    // The caller provisions controllers; kiryl never changes shared ancestors.
    let requested = std::env::var_os("KIRYL_CGROUP_PARENT")
        .map(fs::canonicalize)
        .transpose()?;
    let membership = fs::read_to_string("/proc/self/cgroup")?;
    let current = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| io::Error::other("no unified cgroup membership"))?;
    let current = Path::new(current);
    if current
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(io::Error::other("unresolvable cgroup namespace path"));
    }
    let mounts = fs::read_to_string("/proc/self/mountinfo")?;
    let mut result = Vec::new();
    for line in mounts.lines() {
        let Some((fields, filesystem)) = line.split_once(" - ") else {
            continue;
        };
        if !filesystem.starts_with("cgroup2 ") {
            continue;
        }
        let fields: Vec<_> = fields.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let root = unescape(fields[3])?;
        let mount = unescape(fields[4])?;
        let resolved = if let Ok(relative) = current.strip_prefix(&root) {
            Some(mount.join(relative))
        } else if current == Path::new("/") {
            Some(mount.clone())
        } else {
            None
        };
        if let Some(resolved) = resolved {
            if let Some(parent) = &requested {
                if validate_parent(parent, &mount, &resolved).is_ok() {
                    result.push(parent.clone());
                }
            } else {
                result.push(resolved);
            }
        }
    }
    if requested.is_some() && result.is_empty() {
        return Err(io::Error::other(
            "KIRYL_CGROUP_PARENT is outside this process's cgroup-v2 ancestry",
        ));
    }
    Ok(result)
}
impl Group {
    pub fn attach(child: &Child, memory: u64) -> io::Result<Self> {
        if memory < 4096 {
            return Err(io::Error::other("memory cap smaller than a page"));
        }
        let mut last = io::Error::other("no writable delegated cgroup-v2 mount");
        for parent in parents()? {
            let path = parent.join(format!(
                "kiryl-probe-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => (),
                Err(error) => {
                    last = error;
                    continue;
                }
            }
            let group = Self { path };
            match group.configure(child, memory) {
                Ok(()) => return Ok(group),
                Err(error) => {
                    last = error;
                    drop(group);
                }
            }
        }
        Err(last)
    }
    fn configure(&self, child: &Child, memory: u64) -> io::Result<()> {
        write_existing(&self.path.join("memory.max"), &memory.to_string())?;
        let effective: u64 = fs::read_to_string(self.path.join("memory.max"))?
            .trim()
            .parse()
            .map_err(io::Error::other)?;
        if effective > memory {
            return Err(io::Error::other(
                "kernel rounded memory cap above requested limit",
            ));
        }
        write_existing(&self.path.join("memory.oom.group"), "1")?;
        // Empty-group kill verifies kernel support and permission before attachment.
        write_existing(&self.path.join("cgroup.kill"), "1")?;
        write_existing(&self.path.join("cgroup.procs"), &child.id().to_string())
    }
    pub fn oom_observed(&self) -> io::Result<bool> {
        let counters = fs::read_to_string(self.path.join("memory.events"))?;
        for line in counters.lines() {
            let mut fields = line.split_whitespace();
            if fields.next() == Some("oom_kill") {
                let count: u64 = fields
                    .next()
                    .ok_or_else(|| io::Error::other("missing oom counter"))?
                    .parse()
                    .map_err(io::Error::other)?;
                return Ok(count > 0);
            }
        }
        Err(io::Error::other("missing kernel oom_kill counter"))
    }
    pub fn terminate(&self) -> io::Result<()> {
        write_existing(&self.path.join("cgroup.kill"), "1")?;
        let start = Instant::now();
        while fs::read_to_string(self.path.join("cgroup.events"))?
            .lines()
            .any(|line| line == "populated 1")
        {
            if start.elapsed() > Duration::from_millis(500) {
                return Err(io::Error::other("cgroup cleanup did not quiesce"));
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_parent_must_contain_controller_and_stay_in_mount() {
        let mount = Path::new("/sys/fs/cgroup");
        let current = Path::new("/sys/fs/cgroup/delegated/controller");
        assert!(validate_parent(Path::new("/sys/fs/cgroup/delegated"), mount, current).is_ok());
        assert!(validate_parent(current, mount, current).is_ok());
        assert!(validate_parent(Path::new("/sys/fs/cgroup/other"), mount, current).is_err());
        assert!(validate_parent(Path::new("/sys/fs"), mount, current).is_err());
        assert!(validate_parent(Path::new("relative"), mount, current).is_err());
    }
}
