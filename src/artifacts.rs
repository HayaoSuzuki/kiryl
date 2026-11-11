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
    /// Open a namespace only when its recorded target/environment identity matches.
    /// # Errors
    /// Rejects malformed manifests, unknown schemas and changed identities.
    pub fn open_matching(root: &Path, identity: &serde_json::Value) -> io::Result<Self> {
        let store = Self::open(root)?;
        let path = store.root.join("manifest.json");
        let expected = serde_json::json!({"version": 1, "identity": identity});
        match fs::read(&path) {
            Ok(bytes) => {
                let actual: serde_json::Value = serde_json::from_slice(&bytes)?;
                if actual != expected {
                    return Err(io::Error::other(
                        "incompatible artifact schema or environment identity",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if fs::read_dir(&store.root)?
                    .any(|entry| entry.is_ok_and(|e| e.file_name() != "writer.lock"))
                {
                    return Err(io::Error::other("existing data has no compatible manifest"));
                }
                store.publish_new("manifest.json", &serde_json::to_vec(&expected)?)?;
            }
            Err(error) => return Err(error),
        }
        Ok(store)
    }
    fn publish_new(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let pending = self.root.join(format!("{name}.pending"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        let result = file
            .write_all(bytes)
            .and_then(|()| file.sync_all())
            .and_then(|()| fs::hard_link(&pending, self.root.join(name)));
        drop(file);
        let _ = fs::remove_file(pending);
        result
    }
    /// Save a bounded corpus snapshot; a failed publication leaves prior snapshots.
    /// # Errors
    /// Rejects more than 256 entries or inputs larger than 4096 bytes.
    pub fn save_corpus(&mut self, corpus: &[Vec<u8>]) -> io::Result<()> {
        Self::validate_corpus(corpus)?;
        let bytes = serde_json::to_vec(corpus)?;
        let pending = self.root.join("corpus.pending");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        let result = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| fs::rename(&pending, self.root.join("corpus.json")));
        if result.is_err() {
            let _ = fs::remove_file(pending);
        }
        result
    }
    /// Load a previously saved bounded snapshot; absent corpus is empty.
    /// # Errors
    /// Rejects corrupt, oversized or incompatible input data.
    pub fn load_corpus(&self) -> io::Result<Vec<Vec<u8>>> {
        let path = self.root.join("corpus.json");
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() > 5 * 1024 * 1024 {
            return Err(io::Error::other("corpus file too large"));
        }
        let corpus: Vec<Vec<u8>> = serde_json::from_reader(file)?;
        Self::validate_corpus(&corpus)?;
        Ok(corpus)
    }
    fn validate_corpus(corpus: &[Vec<u8>]) -> io::Result<()> {
        if corpus.len() > 256 || corpus.iter().any(|input| input.len() > 4096) {
            return Err(io::Error::other("corpus exceeds limits"));
        }
        Ok(())
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
