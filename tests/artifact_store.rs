use kiryl::artifacts::ArtifactStore;
use std::fs;

#[test]
fn originals_are_immutable_and_derivatives_are_separate() {
    let root = std::env::temp_dir().join(format!("kiryl-store-{}", std::process::id()));
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
    let root = std::env::temp_dir().join(format!("kiryl-errors-{}", std::process::id()));
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
    let root = std::env::temp_dir().join(format!("kiryl-invalid-{}", std::process::id()));
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
    let root = std::env::temp_dir().join(format!("kiryl-manifest-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let identity = serde_json::json!({"target":"test_x.py::test_x[a]", "python":"3.14"});
    let mut store = ArtifactStore::open_matching(&root, &identity).unwrap();
    let original = store.save_original(b"original").unwrap();
    drop(store);
    assert!(ArtifactStore::open_matching(&root, &serde_json::json!({"target":"other"})).is_err());
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
    let root = std::env::temp_dir().join(format!("kiryl-corpus-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let mut store = ArtifactStore::open(&root).unwrap();
    store.save_corpus(&[vec![0, 255], vec![]]).unwrap();
    assert_eq!(store.load_corpus().unwrap(), vec![vec![0, 255], vec![]]);
    assert!(store.save_corpus(&vec![vec![1]; 257]).is_err());
    assert_eq!(store.load_corpus().unwrap(), vec![vec![0, 255], vec![]]);
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
