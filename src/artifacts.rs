//! Exclusive artifact storage preserving original bytes across worker failures.
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// Owns a single writer namespace. A stale lock is deliberately not removed
/// automatically: another campaign may still own it.
pub struct ArtifactStore {
    root: PathBuf,
    lock: PathBuf,
    next: u64,
}
/// File families have distinct admission and lineage rules.
#[derive(Clone, Copy)]
enum ArtifactKind<'a> {
    Finding,
    Evidence,
    Original,
    Derived(&'a str),
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
        let expected = serde_json::json!({"version": 1, "identity": identity});
        let expected_bytes = serde_json::to_vec(&expected)?;
        if expected_bytes.len() > 1024 * 1024 {
            return Err(io::Error::other("manifest exceeds limits"));
        }
        let store = Self::open(root)?;
        let path = store.root.join("manifest.json");
        match read_bounded(&path, 1024 * 1024) {
            Ok(bytes) => {
                let actual: serde_json::Value = serde_json::from_slice(&bytes)?;
                if actual != expected {
                    return Err(io::Error::other(
                        "incompatible artifact schema or environment identity",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                for entry in fs::read_dir(&store.root)? {
                    if entry?.file_name() != "writer.lock" {
                        return Err(io::Error::other("existing data has no compatible manifest"));
                    }
                }
                store.publish_new("manifest.json", &expected_bytes)?;
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
        let bytes = match read_bounded(&path, 5 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(error),
        };
        let corpus: Vec<Vec<u8>> = serde_json::from_slice(&bytes)?;
        Self::validate_corpus(&corpus)?;
        Ok(corpus)
    }
    fn validate_corpus(corpus: &[Vec<u8>]) -> io::Result<()> {
        if corpus.len() > 256 || corpus.iter().any(|input| input.len() > 4096) {
            return Err(io::Error::other("corpus exceeds limits"));
        }
        Ok(())
    }
    fn save(&mut self, bytes: &[u8], kind: ArtifactKind<'_>) -> io::Result<PathBuf> {
        let prefix = match kind {
            ArtifactKind::Finding => "finding".into(),
            ArtifactKind::Evidence => "evidence".into(),
            ArtifactKind::Original => "original".into(),
            ArtifactKind::Derived(original) => format!("derived-{original}"),
        };
        loop {
            let name = format!("{prefix}-{}.bin", self.next);
            let path = self.root.join(&name);
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| io::Error::other("artifact IDs exhausted"))?;
            match self.publish_new(&name, bytes) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.exists() => (),
                Err(error) => return Err(error),
            }
        }
    }
    /// Atomically preserve the active input and its initial observation.
    /// # Errors
    /// Rejects empty/oversized test IDs, oversized inputs or records and write failures.
    pub fn record_original(
        &mut self,
        test_id: &str,
        input: &[u8],
        observation: &serde_json::Value,
    ) -> io::Result<String> {
        if self.root.to_str().is_none() {
            return Err(io::Error::other("artifact path is not UTF-8"));
        }
        if test_id.is_empty() || test_id.len() > 16 * 1024 || input.len() > 4096 {
            return Err(io::Error::other("original metadata exceeds limits"));
        }
        let record = serde_json::json!({"version":1,"test_id":test_id,"input":input,"observation":observation});
        let bytes = serde_json::to_vec(&record)?;
        if bytes.len() > 1024 * 1024 {
            return Err(io::Error::other("original record exceeds limits"));
        }
        let path = self.save(&bytes, ArtifactKind::Finding)?;
        // The root was validated as UTF-8 above; save adds only an ASCII name.
        Ok(path.to_string_lossy().into_owned())
    }
    /// Append evidence linked to a validated structured original; never replace it.
    /// # Errors
    /// Rejects foreign paths, non-original records, unknown schemas and oversized evidence.
    pub fn derived(&mut self, original: &str, evidence: &serde_json::Value) -> io::Result<String> {
        let original = self.load_original(original)?;
        let test_id = &original.record["test_id"];
        let name = original.name;
        // JSON Value formatting is infallible; all arbitrary serialization has
        // already completed before this persistence boundary.
        let bytes =
            serde_json::json!({"version":1,"original":name,"test_id":test_id,"evidence":evidence})
                .to_string()
                .into_bytes();
        if bytes.len() > 1024 * 1024 {
            return Err(io::Error::other("derived evidence exceeds limits"));
        }
        // The validated original is a UTF-8 path whose parent equals this root;
        // save adds only an ASCII name, preserving that representation.
        Ok(self
            .save(&bytes, ArtifactKind::Evidence)?
            .to_string_lossy()
            .into_owned())
    }
    /// Validate and load an original before starting a replay worker.
    /// # Errors
    /// Rejects foreign, oversized, malformed or unsupported original records.
    fn load_original<'a>(&self, original: &'a str) -> io::Result<ValidatedOriginal<'a>> {
        read_original(&self.root, original)
    }
    /// Save exact active input before attempting verification or shrinking.
    /// # Errors
    /// Returns storage failures; no existing record is overwritten.
    pub fn save_original(&mut self, bytes: &[u8]) -> io::Result<PathBuf> {
        self.save(bytes, ArtifactKind::Original)
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
        self.save(bytes, ArtifactKind::Derived(name))
    }
}
impl Drop for ArtifactStore {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock);
    }
}

fn read_bounded(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((limit as u64) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other("artifact file exceeds limits"));
    }
    Ok(bytes)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayManifest {
    version: u8,
    identity: ReplayIdentity,
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ReplayIdentity {
    target: String,
    environment: serde_json::Value,
}

/// An admitted original and its reproduction context, holding exclusive campaign
/// ownership until dropped. Callers need no manifest filenames or JSON paths.
pub struct ReplayContext {
    original: String,
    directory: PathBuf,
    identity: serde_json::Value,
    environment: serde_json::Value,
    selection: Vec<String>,
    _campaign_lock: ArtifactStore,
}
impl ReplayContext {
    /// Validate a saved original and acquire its campaign before collecting tests.
    /// # Errors
    /// Rejects unsupported formats, inconsistent target identities and active writers.
    pub fn open(original: &Path) -> io::Result<Self> {
        let original = fs::canonicalize(original)?;
        let directory = original
            .parent()
            .ok_or_else(|| io::Error::other("finding has no store"))?
            .to_owned();
        let root = directory
            .parent()
            .ok_or_else(|| io::Error::other("missing campaign store"))?;
        let campaign_lock = ArtifactStore::open(root)?;
        let manifest: ReplayManifest = serde_json::from_slice(&read_bounded(
            &directory.join("manifest.json"),
            1024 * 1024,
        )?)?;
        if manifest.version != 1 {
            return Err(io::Error::other("incompatible artifact schema"));
        }
        let selection = serde_json::from_value(manifest.identity.environment["selection"].clone())?;
        let identity = serde_json::to_value(&manifest.identity)?;
        let store = ArtifactStore::open_matching(&directory, &identity)?;
        let original = original
            .to_str()
            .ok_or_else(|| io::Error::other("non UTF-8 finding path"))?
            .to_owned();
        let record = store.load_original(&original)?;
        if record.record["test_id"] != manifest.identity.target {
            return Err(io::Error::other(
                "finding target does not match store identity",
            ));
        }
        drop(store);
        Ok(Self {
            original,
            directory,
            identity,
            environment: manifest.identity.environment,
            selection,
            _campaign_lock: campaign_lock,
        })
    }
    /// The pytest selection that originally established this finding's identity.
    #[must_use]
    pub fn selection(&self) -> &[String] {
        &self.selection
    }
    /// Admit freshly collected source/dependency identity before worker execution.
    /// # Errors
    /// Returns a mismatch without changing any saved evidence.
    pub fn check_environment(&self, current: &serde_json::Value) -> io::Result<()> {
        if &self.environment != current {
            return Err(io::Error::other(
                "source, selection or environment mismatch",
            ));
        }
        Ok(())
    }
    /// Validated UTF-8 original path within the locked reproduction namespace.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }
    /// Namespace used by the executor for write-once derivative evidence.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Opaque comparison token; only storage interprets its representation.
    #[must_use]
    pub fn store_identity(&self) -> &serde_json::Value {
        &self.identity
    }
}

// The validated basename travels with its parsed record so derivative storage
// does not repeat path validation or manufacture another missing-name failure.
struct ValidatedOriginal<'a> {
    name: &'a str,
    record: serde_json::Value,
}

fn read_original<'a>(root: &Path, original: &'a str) -> io::Result<ValidatedOriginal<'a>> {
    let path = Path::new(original);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("invalid original path"))?;
    if path.parent() != Some(root)
        || name
            .strip_prefix("finding-")
            .and_then(|s| s.strip_suffix(".bin"))
            .and_then(|s| s.parse::<u64>().ok())
            .is_none()
        || !fs::symlink_metadata(path)?.is_file()
    {
        return Err(io::Error::other(
            "expected structured original in this store",
        ));
    }
    let record: serde_json::Value = serde_json::from_slice(&read_bounded(path, 1024 * 1024)?)?;
    let _test_id = record
        .get("test_id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 16 * 1024)
        .ok_or_else(|| io::Error::other("invalid original test metadata"))?;
    let input: Vec<u8> = serde_json::from_value(
        record
            .get("input")
            .cloned()
            .ok_or_else(|| io::Error::other("missing original input"))?,
    )?;
    if record.get("version").and_then(serde_json::Value::as_u64) != Some(1)
        || input.len() > 4096
        || record.get("observation").is_none()
    {
        return Err(io::Error::other("incompatible original record"));
    }
    Ok(ValidatedOriginal { name, record })
}

/// Available finding values, independent of the persisted record layout.
pub struct FindingDescription {
    pub rendered: Option<String>,
    pub fingerprint: Option<String>,
    pub replayable: bool,
}
/// Read validated immutable evidence for user presentation without a writer lock.
/// # Errors
/// Rejects unsupported, foreign, malformed or oversized original records.
pub fn describe_original(path: &Path) -> io::Result<FindingDescription> {
    #[derive(serde::Deserialize)]
    struct ObservationDescription {
        rendered: Option<String>,
        fingerprint: Option<String>,
        replay_token: Option<Vec<u8>>,
    }
    let root = path
        .parent()
        .ok_or_else(|| io::Error::other("finding has no store"))?;
    let record = read_original(
        root,
        path.to_str()
            .ok_or_else(|| io::Error::other("non UTF-8 finding path"))?,
    )?;
    let observation: ObservationDescription =
        serde_json::from_value(record.record["observation"].clone())?;
    Ok(FindingDescription {
        rendered: observation.rendered,
        fingerprint: observation.fingerprint,
        replayable: observation.replay_token.is_some(),
    })
}

/// Replay data admitted from storage; record envelopes remain storage-owned.
#[derive(serde::Deserialize)]
pub struct SavedFinding {
    pub test_id: String,
    pub input: Vec<u8>,
    pub observation: FindingObservation,
}
/// Semantic failure evidence, independent of the original record's envelope.
#[derive(serde::Deserialize)]
pub struct FindingObservation {
    pub kind: crate::executor::OutcomeKind,
    pub fingerprint: Option<String>,
    pub replay_token: Option<Vec<u8>>,
    pub rendered: Option<String>,
}
impl ArtifactStore {
    /// Return validated replay inputs without exposing the persistence layout.
    /// # Errors
    /// Rejects incompatible records or malformed semantic evidence.
    pub fn load_finding(&self, original: &str) -> io::Result<SavedFinding> {
        Ok(serde_json::from_value(
            self.load_original(original)?.record,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn non_utf8_store_rejects_original_before_allocating_an_id() {
        use std::os::unix::ffi::OsStringExt;
        let mut store = ArtifactStore {
            root: PathBuf::from(std::ffi::OsString::from_vec(vec![0xff])),
            lock: PathBuf::new(),
            next: 0,
        };
        assert_eq!(
            store
                .record_original("test", &[1], &serde_json::json!({}))
                .unwrap_err()
                .to_string(),
            "artifact path is not UTF-8"
        );
        assert_eq!(store.next, 0);
    }

    #[test]
    fn exhausted_ids_fail_before_publishing_or_wrapping() {
        let mut store = ArtifactStore {
            root: PathBuf::from("unreached-artifact-destination"),
            lock: PathBuf::new(),
            next: u64::MAX,
        };
        assert_eq!(
            store
                .save_original(b"new original")
                .unwrap_err()
                .to_string(),
            "artifact IDs exhausted"
        );
        assert_eq!(store.next, u64::MAX);
    }
}
