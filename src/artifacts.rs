//! Exclusive artifact storage preserving original bytes across worker failures.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// Owns a single writer namespace. A stale lock is deliberately not removed
/// automatically: another campaign may still own it.
pub struct ArtifactStore {
    root: PathBuf,
    lock: PathBuf,
    next: u64,
}
impl ArtifactStore {
    /// Acquire exclusive access without changing existing records.
    /// # Errors
    /// Returns filesystem errors, including an existing campaign lock.
    pub fn open(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let lock = root.join("writer.lock");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)?;
        Ok(Self {
            root,
            lock,
            next: 0,
        })
    }
    fn save(&mut self, bytes: &[u8], kind: &str) -> io::Result<PathBuf> {
        loop {
            let path = self.root.join(format!("{kind}-{}.bin", self.next));
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| io::Error::other("artifact IDs exhausted"))?;
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                        drop(file);
                        let _ = fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
    }
    /// Save exact active input before attempting verification or shrinking.
    /// # Errors
    /// Returns storage failures; no existing record is overwritten.
    pub fn save_original(&mut self, bytes: &[u8]) -> io::Result<PathBuf> {
        self.save(bytes, "original")
    }
    /// Save a separate candidate linked by its original's unique filename.
    /// # Errors
    /// Rejects originals outside this store or missing original files.
    pub fn save_derivative(&mut self, original: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
        if original.parent() != Some(self.root.as_path()) {
            return Err(io::Error::other("original outside store"));
        }
        if !fs::symlink_metadata(original)?.is_file() {
            return Err(io::Error::other("original must be a regular file"));
        }
        let name = original
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| io::Error::other("invalid original"))?;
        if name
            .strip_prefix("original-")
            .and_then(|v| v.strip_suffix(".bin"))
            .and_then(|v| v.parse::<u64>().ok())
            .is_none()
        {
            return Err(io::Error::other("expected original"));
        }
        self.save(bytes, &format!("derived-{name}"))
    }
}
impl Drop for ArtifactStore {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock);
    }
}
