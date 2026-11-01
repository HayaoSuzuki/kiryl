// Copyright (c) 2026 kiryl contributors
//! Write-once original records and separate, linked derived evidence.
use crate::controller::Observation;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub type FindingId = String;
pub type StoreError = io::Error;

pub struct ArtifactStore {
    root: PathBuf,
    next: u64,
}
impl ArtifactStore {
    /// Open an output namespace; existing records are never replaced.
    /// # Errors
    /// Fails if the directory cannot be created.
    pub fn new(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_owned(),
            next: 0,
        })
    }
    fn create(&mut self, value: &Value) -> io::Result<String> {
        loop {
            let path = self.root.join(format!("record-{}.json", self.next));
            self.next += 1;
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    // No observer receives this path until the complete write succeeds.
                    if let Err(error) = file.write_all(&serde_json::to_vec(value)?) {
                        drop(file);
                        let _ = fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(path.to_string_lossy().into_owned());
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
    }
    /// Persist raw input before acknowledging a failure to the worker.
    /// # Errors
    /// Returns storage failures without initiating replay or minimization.
    pub fn record_original(
        &mut self,
        test_id: &str,
        input: &[u8],
        observation: &Observation,
    ) -> Result<FindingId, StoreError> {
        self.create(
            &json!({"version": 1, "test_id": test_id, "input": input, "observation": observation}),
        )
    }
    /// Append derived evidence without changing an original, including on failure.
    /// # Errors
    /// Rejects incompatible originals or reports write failures.
    pub fn derived(&mut self, original: &str, evidence: &Value) -> io::Result<String> {
        let source: Value = serde_json::from_slice(&fs::read(original)?)?;
        if source["version"].as_u64() != Some(1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incompatible artifact",
            ));
        }
        self.create(&json!({"version": 1, "original": original, "evidence": evidence}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn derived_never_changes_original(input in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..1024), values in proptest::collection::vec(proptest::prelude::any::<u64>(), 0..32)) {
            let temporary = tempfile::tempdir().unwrap();
            let mut store = ArtifactStore::new(temporary.path()).unwrap();
            let original = store.record_original("test", &input, &Observation::default()).unwrap();
            let bytes = fs::read(&original).unwrap();
            for value in values {
                store.derived(&original, &json!({"value": value})).unwrap();
                let current = fs::read(&original).unwrap();
                prop_assert_eq!(&current, &bytes);
            }
            // A reopened store must not overwrite preexisting numeric IDs.
            let mut reopened = ArtifactStore::new(temporary.path()).unwrap();
            let second = reopened.record_original("other", &[], &Observation::default()).unwrap();
            prop_assert_ne!(second, original.clone());
            prop_assert_eq!(fs::read(&original).unwrap(), bytes);
        }
    }
    #[test]
    fn bad_version_and_derived_write_failure_preserve_original() {
        let temporary = tempfile::tempdir().unwrap();
        let original = temporary.path().join("source.json");
        for version in [json!(2), json!(true), json!(1.0)] {
            fs::write(
                &original,
                serde_json::to_vec(&json!({"version": version})).unwrap(),
            )
            .unwrap();
            let before = fs::read(&original).unwrap();
            let mut store = ArtifactStore::new(&temporary.path().join("derived")).unwrap();
            assert!(
                store
                    .derived(original.to_str().unwrap(), &json!({}))
                    .is_err()
            );
            assert_eq!(fs::read(&original).unwrap(), before);
        }
        fs::write(&original, br#"{"version":1}"#).unwrap();
        let output = temporary.path().join("missing");
        let mut store = ArtifactStore::new(&output).unwrap();
        fs::remove_dir(&output).unwrap();
        assert!(
            store
                .derived(original.to_str().unwrap(), &json!({}))
                .is_err()
        );
        assert_eq!(fs::read(original).unwrap(), br#"{"version":1}"#);
    }
}
