use kiryl::artifacts::ArtifactStore;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn temporary_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kiryl-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn originals_are_immutable_and_derivatives_are_separate() {
    let root = temporary_root("store");
    let _ = fs::remove_dir_all(&root);
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store.save_original(b"exact input").unwrap();
    let derived = store.save_derivative(&original, b"smaller input").unwrap();
    assert_ne!(original, derived);
    assert_eq!(fs::read(&original).unwrap(), b"exact input");
    assert_eq!(fs::read(&derived).unwrap(), b"smaller input");
    assert!(ArtifactStore::open(&root).is_err());
    drop(store);
    let mut reopened = ArtifactStore::open(&root).unwrap();
    let next = reopened.save_original(b"new").unwrap();
    assert_ne!(original, next);
    assert_eq!(fs::read(original).unwrap(), b"exact input");
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_derivatives_and_failed_save_preserve_original() {
    let root = temporary_root("errors");
    let _ = fs::remove_dir_all(&root);
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store.save_original(&[0, 255, 128]).unwrap();
    assert!(store.save_derivative(&root.join("missing"), b"x").is_err());
    assert!(
        store
            .save_derivative(&root.join("../foreign"), b"x")
            .is_err()
    );
    let derived = store.save_derivative(&original, b"").unwrap();
    assert!(store.save_derivative(&derived, b"x").is_err());
    // Removing the directory simulates loss of the writable destination.
    let retained = fs::read(&original).unwrap();
    let moved = root.with_extension("moved");
    fs::rename(&root, &moved).unwrap();
    assert!(store.save_original(b"new").is_err());
    assert_eq!(
        fs::read(moved.join(original.file_name().unwrap())).unwrap(),
        retained
    );
    fs::rename(&moved, &root).unwrap();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn directories_and_malformed_names_are_not_originals() {
    let root = temporary_root("invalid");
    fs::create_dir_all(&root).unwrap();
    let mut store = ArtifactStore::open(&root).unwrap();
    let directory = root.join("original-42.bin");
    fs::create_dir(&directory).unwrap();
    assert!(store.save_derivative(&directory, b"x").is_err());
    let malformed = root.join("original-fake.bin");
    fs::write(&malformed, b"x").unwrap();
    assert!(store.save_derivative(&malformed, b"x").is_err());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn environment_mismatch_and_unknown_schema_preserve_store() {
    let root = temporary_root("manifest");
    let _ = fs::remove_dir_all(&root);
    let identity = serde_json::json!({"target":"test_x.py::test_x[a]", "python":"3.14"});
    let mut store = ArtifactStore::open_matching(&root, &identity).unwrap();
    let original = store.save_original(b"original").unwrap();
    drop(store);
    assert!(
        ArtifactStore::open_matching(
            &root,
            &serde_json::json!({"target":"test_x.py::test_x[b]", "python":"3.14"})
        )
        .is_err()
    );
    assert_eq!(fs::read(&original).unwrap(), b"original");
    drop(ArtifactStore::open_matching(&root, &identity).unwrap());
    fs::write(
        root.join("manifest.json"),
        br#"{"version":2,"identity":{}}"#,
    )
    .unwrap();
    assert!(ArtifactStore::open_matching(&root, &identity).is_err());
    assert_eq!(fs::read(original).unwrap(), b"original");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn corpus_is_bounded_and_round_trips() {
    let root = temporary_root("corpus");
    let _ = fs::remove_dir_all(&root);
    let mut store = ArtifactStore::open(&root).unwrap();
    store.save_corpus(&[vec![0, 255], vec![]]).unwrap();
    assert_eq!(store.load_corpus().unwrap(), vec![vec![0, 255], vec![]]);
    assert!(store.save_corpus(&vec![vec![1]; 257]).is_err());
    assert_eq!(store.load_corpus().unwrap(), vec![vec![0, 255], vec![]]);
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn structured_originals_are_complete_and_derived_evidence_is_separate() {
    let root = temporary_root("records");
    let mut store = ArtifactStore::open(&root).unwrap();
    let observation = serde_json::json!({"kind":"crash", "signal":11});
    let original = store
        .record_original("tests/test_x.py::test_x[a]", &[0, 255], &observation)
        .unwrap();
    let bytes = fs::read(&original).unwrap();
    let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        record,
        serde_json::json!({"version":1,"test_id":"tests/test_x.py::test_x[a]","input":[0,255],"observation":observation})
    );
    let derived = store
        .derived(&original, &serde_json::json!({"input":[]}))
        .unwrap();
    assert_ne!(derived, original);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(store.derived(&derived, &serde_json::json!({})).is_err());
    let raw = store.save_original(b"raw").unwrap();
    assert!(
        store
            .derived(raw.to_str().unwrap(), &serde_json::json!({}))
            .is_err()
    );
    fs::write(
        &original,
        br#"{"version":2,"test_id":"x","input":[],"observation":{}}"#,
    )
    .unwrap();
    assert!(store.derived(&original, &serde_json::json!({})).is_err());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pending_failures_and_bounds_preserve_prior_records() {
    let root = temporary_root("failure");
    let mut store = ArtifactStore::open(&root).unwrap();
    store.save_corpus(&[vec![7]]).unwrap();
    fs::write(root.join("corpus.pending"), b"interrupted").unwrap();
    assert!(store.save_corpus(&[vec![8]]).is_err());
    assert_eq!(store.load_corpus().unwrap(), vec![vec![7]]);
    assert!(store.save_corpus(&[vec![0; 4097]]).is_err());
    assert!(
        store
            .record_original("", &[], &serde_json::json!({}))
            .is_err()
    );
    assert!(
        store
            .record_original("x", &vec![0; 4097], &serde_json::json!({}))
            .is_err()
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incompatible_metadata_and_oversized_manifests_are_rejected_without_data_loss() {
    let root = temporary_root("metadata");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original("x[a]", &[1], &serde_json::json!({}))
        .unwrap();
    for record in [
        serde_json::json!({"version":1,"test_id":"","input":[],"observation":{}}),
        serde_json::json!({"version":1,"test_id":"x","input":[256],"observation":{}}),
        serde_json::json!({"version":1,"test_id":"x","input":[]}),
    ] {
        fs::write(&original, serde_json::to_vec(&record).unwrap()).unwrap();
        let before = fs::read(&original).unwrap();
        assert!(store.derived(&original, &serde_json::json!({})).is_err());
        assert_eq!(fs::read(&original).unwrap(), before);
    }
    drop(store);
    assert!(ArtifactStore::open_matching(&root, &serde_json::json!({})).is_err());
    assert!(!root.join("writer.lock").exists());
    fs::write(root.join("manifest.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(ArtifactStore::open_matching(&root, &serde_json::json!({})).is_err());
    assert!(std::path::Path::new(&original).exists());
    assert!(!root.join("writer.lock").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_original_publication_is_never_visible_as_a_finding() {
    let root = temporary_root("pending-original");
    let mut store = ArtifactStore::open(&root).unwrap();
    fs::write(root.join("finding-0.bin.pending"), b"partial write").unwrap();
    assert!(
        store
            .record_original("x", &[1], &serde_json::json!({}))
            .is_err()
    );
    assert!(!root.join("finding-0.bin").exists());
    assert_eq!(
        fs::read(root.join("finding-0.bin.pending")).unwrap(),
        b"partial write"
    );
    let next = store
        .record_original("x", &[2], &serde_json::json!({}))
        .unwrap();
    assert!(std::path::Path::new(&next).exists());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn corpus_boundary_and_corrupt_load_do_not_repair_files() {
    let root = temporary_root("corpus-boundary");
    let mut store = ArtifactStore::open(&root).unwrap();
    assert_eq!(store.load_corpus().unwrap(), [] as [Vec<u8>; 0]);
    let maximum = vec![vec![255; 4096]; 256];
    store.save_corpus(&maximum).unwrap();
    assert_eq!(store.load_corpus().unwrap(), maximum);
    for bytes in [b"broken".as_slice(), b"[[256]]".as_slice()] {
        fs::write(root.join("corpus.json"), bytes).unwrap();
        assert!(store.load_corpus().is_err());
        assert_eq!(fs::read(root.join("corpus.json")).unwrap(), bytes);
    }
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_cannot_be_used_as_an_original() {
    let root = temporary_root("symlink");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original("x", &[1], &serde_json::json!({}))
        .unwrap();
    let link = root.join("finding-99.bin");
    std::os::unix::fs::symlink(&original, &link).unwrap();
    assert!(
        store
            .derived(link.to_str().unwrap(), &serde_json::json!({}))
            .is_err()
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
