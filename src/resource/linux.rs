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
    let current = membership_path(&membership)?;
    let mounts = fs::read_to_string("/proc/self/mountinfo")?;
    resolve_parents(requested.as_deref(), current, &mounts)
}

fn membership_path(membership: &str) -> io::Result<&Path> {
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
    Ok(current)
}

fn resolve_parents(
    requested: Option<&Path>,
    current: &Path,
    mounts: &str,
) -> io::Result<Vec<PathBuf>> {
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
                    result.push(parent.to_path_buf());
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
        Self::attach_in(child, memory, parents()?)
    }
    fn attach_in(child: &Child, memory: u64, parents: Vec<PathBuf>) -> io::Result<Self> {
        let mut last = io::Error::other("no writable delegated cgroup-v2 mount");
        for parent in parents {
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

    use proptest::prelude::*;
    use std::os::unix::ffi::OsStrExt;

    struct Files {
        root: PathBuf,
    }
    impl Files {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "kiryl-cgroup-files-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self { root }
        }
        fn write(&self, name: &str, text: &str) {
            fs::write(self.root.join(name), text).unwrap();
        }
        fn group(&self) -> Group {
            Group {
                path: self.root.clone(),
            }
        }
        fn child() -> Child {
            std::process::Command::new("/bin/sleep")
                .arg("30")
                .spawn()
                .unwrap()
        }
    }
    impl Drop for Files {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn terminate_child(mut child: Child) {
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[rstest::rstest]
    #[case("/a\\040b", b"/a b".as_slice())]
    #[case("/a\\134b", b"/a\\b".as_slice())]
    #[case("/a\\000b", b"/a\0b".as_slice())]
    #[case("/plain", b"/plain".as_slice())]
    fn mount_octal_escape_decodes(#[case] input: &str, #[case] expected: &[u8]) {
        assert_eq!(unescape(input).unwrap().as_os_str().as_bytes(), expected);
    }
    #[rstest::rstest]
    fn malformed_mount_escape_fails(#[values("\\", "\\0", "\\00", "\\008", "\\777")] input: &str) {
        assert_eq!(unescape(input).unwrap_err().to_string(), "bad mount escape");
    }
    proptest! {
        #[test]
        fn arbitrary_byte_octal_escape_round_trips(byte in any::<u8>()) {
            let input = format!("\\{byte:03o}");
            let decoded = unescape(&input).unwrap();
            prop_assert_eq!(decoded.as_os_str().as_bytes(), &[byte]);
        }
    }
    #[rstest::rstest]
    #[case("1:cpu:/", "no unified cgroup membership")]
    #[case("0::/../host", "unresolvable cgroup namespace path")]
    fn invalid_membership_is_rejected(#[case] membership: &str, #[case] expected: &str) {
        assert_eq!(
            membership_path(membership).unwrap_err().to_string(),
            expected
        );
    }
    #[rstest::rstest]
    #[case(None, "0::/team/job", "1 2 0:1 / /cg x - cgroup2 cgroup rw", vec!["/cg/team/job"])]
    #[case(None, "0::/", "1 2 0:1 /delegated /cg x - cgroup2 cgroup rw", vec!["/cg"])]
    #[case(None, "0::/other", "1 2 0:1 /delegated /cg x - cgroup2 cgroup rw", vec![])]
    #[case(None, "0::/", "invalid\n1 2 0:1 / /old x - tmpfs tmpfs rw\n1 2 - cgroup2 x rw", vec![])]
    #[case(Some("/cg/team"), "0::/team/job", "1 2 0:1 / /cg x - cgroup2 cgroup rw", vec!["/cg/team"])]
    #[case(None, "1:cpu:/ignored\n0::/team", "1 2 0:1 / /a\\040b x - cgroup2 cgroup rw", vec!["/a b/team"])]
    fn mount_resolution_uses_namespace_and_ancestry(
        #[case] requested: Option<&str>,
        #[case] membership: &str,
        #[case] mounts: &str,
        #[case] expected: Vec<&str>,
    ) {
        let result = resolve_parents(
            requested.map(Path::new),
            membership_path(membership).unwrap(),
            mounts,
        )
        .unwrap();
        assert_eq!(
            result,
            expected.iter().map(PathBuf::from).collect::<Vec<_>>()
        );
    }
    #[rstest::rstest]
    #[case(
        Some("/elsewhere"),
        "0::/team",
        "1 2 0:1 / /cg x - cgroup2 cgroup rw",
        "KIRYL_CGROUP_PARENT is outside this process's cgroup-v2 ancestry"
    )]
    #[case(
        Some("/cg"),
        "0::/",
        "",
        "KIRYL_CGROUP_PARENT is outside this process's cgroup-v2 ancestry"
    )]
    #[case(
        None,
        "0::/",
        "1 2 0:1 /\\777 /cg x - cgroup2 cgroup rw",
        "bad mount escape"
    )]
    #[case(
        None,
        "0::/",
        "1 2 0:1 / /\\777 x - cgroup2 cgroup rw",
        "bad mount escape"
    )]
    fn invalid_mount_resolution_fails_closed(
        #[case] requested: Option<&str>,
        #[case] membership: &str,
        #[case] mounts: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(
            resolve_parents(
                requested.map(Path::new),
                membership_path(membership).unwrap(),
                mounts
            )
            .unwrap_err()
            .to_string(),
            expected
        );
    }
    #[test]
    fn subpage_memory_cap_fails_before_attachment() {
        let child = Files::child();
        let error = Group::attach(&child, 4095).err().unwrap();
        terminate_child(child);
        assert_eq!(error.to_string(), "memory cap smaller than a page");
    }
    #[test]
    fn empty_parent_list_preserves_unavailable_error() {
        let child = Files::child();
        let result = Group::attach_in(&child, 4096, vec![]);
        terminate_child(child);
        assert_eq!(
            result.err().unwrap().to_string(),
            "no writable delegated cgroup-v2 mount"
        );
    }
    #[test]
    fn owned_file_parent_preserves_directory_creation_error() {
        let files = Files::new();
        files.write("blocked", "owned file");
        let child = Files::child();
        let result = Group::attach_in(&child, 4096, vec![files.root.join("blocked")]);
        terminate_child(child);
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::NotADirectory);
        assert_eq!(
            fs::read_to_string(files.root.join("blocked")).unwrap(),
            "owned file"
        );
    }
    #[test]
    fn failed_configuration_removes_owned_group_and_preserves_error() {
        let files = Files::new();
        let child = Files::child();
        let result = Group::attach_in(&child, 4096, vec![files.root.clone()]);
        terminate_child(child);
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read_dir(&files.root).unwrap().count(), 0);
    }
    #[test]
    fn later_parent_failure_replaces_prior_creation_error() {
        let files = Files::new();
        files.write("blocked", "owned file");
        let parent = files.root.join("candidate");
        fs::create_dir(&parent).unwrap();
        let child = Files::child();
        let result = Group::attach_in(
            &child,
            4096,
            vec![files.root.join("blocked"), parent.clone()],
        );
        terminate_child(child);
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read_dir(parent).unwrap().count(), 0);
        assert_eq!(
            fs::read_to_string(files.root.join("blocked")).unwrap(),
            "owned file"
        );
    }
    #[test]
    fn configuration_writes_owned_controls_and_process() {
        let files = Files::new();
        files.write("memory.max", "");
        files.write("memory.oom.group", "0");
        files.write("cgroup.kill", "0");
        files.write("cgroup.procs", "");
        let child = Files::child();
        let pid = child.id();
        files.group().configure(&child, 4096).unwrap();
        terminate_child(child);
        assert_eq!(
            fs::read_to_string(files.root.join("memory.max")).unwrap(),
            "4096"
        );
        assert_eq!(
            fs::read_to_string(files.root.join("memory.oom.group")).unwrap(),
            "1"
        );
        assert_eq!(
            fs::read_to_string(files.root.join("cgroup.kill")).unwrap(),
            "1"
        );
        assert_eq!(
            fs::read_to_string(files.root.join("cgroup.procs")).unwrap(),
            pid.to_string()
        );
    }
    fn effective_memory_peer(path: PathBuf, response: Vec<u8>) -> thread::JoinHandle<String> {
        use std::io::Read;
        let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: cpath is a live NUL-terminated owned path; mode grants owner access.
        assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
        thread::spawn(move || {
            let mut requested = String::new();
            fs::File::open(&path)
                .unwrap()
                .read_to_string(&mut requested)
                .unwrap();
            OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .write_all(&response)
                .unwrap();
            requested
        })
    }
    #[rstest::rstest]
    #[case("not-a-number", "invalid digit")]
    #[case("8192", "kernel rounded memory cap above requested limit")]
    fn unacceptable_effective_memory_limit_is_rejected(
        #[case] response: &str,
        #[case] expected: &str,
    ) {
        let files = Files::new();
        let peer =
            effective_memory_peer(files.root.join("memory.max"), response.as_bytes().to_vec());
        let child = Files::child();
        let result = files.group().configure(&child, 4096);
        terminate_child(child);
        assert_eq!(peer.join().unwrap(), "4096");
        assert!(result.unwrap_err().to_string().contains(expected));
    }
    #[test]
    fn non_utf8_effective_memory_limit_is_rejected() {
        let files = Files::new();
        let peer = effective_memory_peer(files.root.join("memory.max"), vec![255]);
        let child = Files::child();
        let result = files.group().configure(&child, 4096);
        terminate_child(child);
        assert_eq!(peer.join().unwrap(), "4096");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
    #[rstest::rstest]
    fn missing_configuration_control_fails(
        #[values("memory.max", "memory.oom.group", "cgroup.kill", "cgroup.procs")] missing: &str,
    ) {
        let files = Files::new();
        files.write("memory.max", "");
        files.write("memory.oom.group", "0");
        files.write("cgroup.kill", "0");
        files.write("cgroup.procs", "");
        fs::remove_file(files.root.join(missing)).unwrap();
        let child = Files::child();
        let result = files.group().configure(&child, 4096);
        terminate_child(child);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }
    #[rstest::rstest]
    #[case("low 0\noom_kill 0\n", false)]
    #[case("oom_kill 2\n", true)]
    fn kernel_oom_counter_is_observed(#[case] counters: &str, #[case] expected: bool) {
        let files = Files::new();
        files.write("memory.events", counters);
        assert_eq!(files.group().oom_observed().unwrap(), expected);
    }
    #[rstest::rstest]
    #[case("oom_kill\n", "missing oom counter")]
    #[case("oom_kill invalid\n", "invalid digit")]
    #[case("oom 1\n", "missing kernel oom_kill counter")]
    fn invalid_kernel_oom_evidence_is_rejected(#[case] counters: &str, #[case] expected: &str) {
        let files = Files::new();
        files.write("memory.events", counters);
        assert!(
            files
                .group()
                .oom_observed()
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
    #[test]
    fn missing_kernel_oom_file_is_rejected() {
        let files = Files::new();
        assert_eq!(
            files.group().oom_observed().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
    #[test]
    fn empty_group_cleanup_writes_kill() {
        let files = Files::new();
        files.write("cgroup.kill", "0");
        files.write("cgroup.events", "populated 0\n");
        files.group().terminate().unwrap();
        assert_eq!(
            fs::read_to_string(files.root.join("cgroup.kill")).unwrap(),
            "1"
        );
    }
    #[rstest::rstest]
    fn missing_cleanup_control_fails(#[values("cgroup.kill", "cgroup.events")] missing: &str) {
        let files = Files::new();
        files.write("cgroup.kill", "0");
        files.write("cgroup.events", "populated 0\n");
        fs::remove_file(files.root.join(missing)).unwrap();
        assert_eq!(
            files.group().terminate().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
    #[test]
    fn populated_group_cleanup_has_a_deadline() {
        let files = Files::new();
        files.write("cgroup.kill", "0");
        files.write("cgroup.events", "populated 1\n");
        assert_eq!(
            files.group().terminate().unwrap_err().to_string(),
            "cgroup cleanup did not quiesce"
        );
    }
    #[rstest::rstest]
    #[case("/sys/fs/cgroup/delegated", true)]
    #[case("/sys/fs/cgroup/delegated/controller", true)]
    #[case("/sys/fs/cgroup/other", false)]
    #[case("/sys/fs", false)]
    #[case("relative", false)]
    fn explicit_parent_must_contain_controller_and_stay_in_mount(
        #[case] parent: &str,
        #[case] valid: bool,
    ) {
        let mount = Path::new("/sys/fs/cgroup");
        let current = Path::new("/sys/fs/cgroup/delegated/controller");
        assert_eq!(
            validate_parent(Path::new(parent), mount, current).is_ok(),
            valid
        );
    }
}
