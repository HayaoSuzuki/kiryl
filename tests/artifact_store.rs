mod common;
use common::TemporaryRoot;
use kiryl::artifacts::ArtifactStore;
use std::fs;

#[test]
fn derivatives_preserve_original_bytes() {
    let root = TemporaryRoot::new("store");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store.save_original(b"exact input").unwrap();
    let derived = store.save_derivative(&original, b"smaller input").unwrap();
    assert_ne!(original, derived);
    assert_eq!(fs::read(&original).unwrap(), b"exact input");
    assert_eq!(fs::read(&derived).unwrap(), b"smaller input");
}
#[test]
fn active_writer_excludes_second_writer() {
    let root = TemporaryRoot::new("lock");
    let _store = ArtifactStore::open(&root).unwrap();
    assert!(ArtifactStore::open(&root).is_err());
}
#[test]
fn reopening_allocates_new_original_without_overwrite() {
    let root = TemporaryRoot::new("reopen");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store.save_original(b"exact input").unwrap();
    drop(store);
    let mut reopened = ArtifactStore::open(&root).unwrap();
    let next = reopened.save_original(b"new").unwrap();
    assert_ne!(original, next);
    assert_eq!(fs::read(original).unwrap(), b"exact input");
}

#[derive(Clone, Copy)]
enum InvalidSource {
    Missing,
    Foreign,
    Derived,
    Directory,
    Malformed,
}

fn invalid_source(
    root: &std::path::Path,
    store: &mut ArtifactStore,
    original: &std::path::Path,
    scenario: InvalidSource,
) -> std::path::PathBuf {
    match scenario {
        InvalidSource::Missing => root.join("missing"),
        InvalidSource::Foreign => root.join("../foreign"),
        InvalidSource::Derived => store.save_derivative(original, b"").unwrap(),
        InvalidSource::Directory => {
            let path = root.join("original-42.bin");
            fs::create_dir(&path).unwrap();
            path
        }
        InvalidSource::Malformed => {
            let path = root.join("original-fake.bin");
            fs::write(&path, b"x").unwrap();
            path
        }
    }
}
#[rstest::rstest]
#[case::missing(InvalidSource::Missing)]
#[case::foreign(InvalidSource::Foreign)]
#[case::derived(InvalidSource::Derived)]
#[case::directory(InvalidSource::Directory)]
#[case::malformed(InvalidSource::Malformed)]
fn invalid_derivatives_preserve_original(#[case] scenario: InvalidSource) {
    let root = TemporaryRoot::new("invalid-source");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store.save_original(&[0, 255, 128]).unwrap();
    let source = invalid_source(
        &fs::canonicalize(&root).unwrap(),
        &mut store,
        &original,
        scenario,
    );
    assert!(store.save_derivative(&source, b"x").is_err());
    assert_eq!(fs::read(original).unwrap(), [0, 255, 128]);
}
#[test]
fn failed_save_preserves_original() {
    let root = TemporaryRoot::new("failed-save");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store.save_original(&[0, 255, 128]).unwrap();
    let destination = TemporaryRoot::new("retained");
    let moved = destination.join("moved");
    fs::rename(&root, &moved).unwrap();
    assert!(store.save_original(b"new").is_err());
    assert_eq!(
        fs::read(moved.join(original.file_name().unwrap())).unwrap(),
        [0, 255, 128]
    );
    fs::rename(&moved, &root).unwrap();
}

#[rstest::rstest]
#[case::identity_mismatch(false)]
#[case::unknown_schema(true)]
fn incompatible_store_preserves_original(#[case] unknown_schema: bool) {
    let root = TemporaryRoot::new("manifest");
    let identity = serde_json::json!({"target":"test_x.py::test_x[a]", "python":"3.14"});
    let mut store = ArtifactStore::open_matching(&root, &identity).unwrap();
    let original = store.save_original(b"original").unwrap();
    drop(store);
    let requested = incompatible_manifest(&root, &identity, unknown_schema);
    assert!(ArtifactStore::open_matching(&root, &requested).is_err());
    assert_eq!(fs::read(original).unwrap(), b"original");
}
fn incompatible_manifest(
    root: &std::path::Path,
    identity: &serde_json::Value,
    unknown_schema: bool,
) -> serde_json::Value {
    if unknown_schema {
        drop(ArtifactStore::open_matching(root, identity).unwrap());
        fs::write(
            root.join("manifest.json"),
            br#"{"version":2,"identity":{}}"#,
        )
        .unwrap();
        identity.clone()
    } else {
        serde_json::json!({"target":"test_x.py::test_x[b]", "python":"3.14"})
    }
}

#[test]
fn corpus_round_trips() {
    let root = TemporaryRoot::new("corpus");
    let mut store = ArtifactStore::open(&root).unwrap();
    store.save_corpus(&[vec![0, 255], vec![]]).unwrap();
    assert_eq!(store.load_corpus().unwrap(), vec![vec![0, 255], vec![]]);
}
#[test]
fn oversized_corpus_preserves_previous_corpus() {
    let root = TemporaryRoot::new("bounded-corpus");
    let mut store = ArtifactStore::open(&root).unwrap();
    store.save_corpus(&[vec![0, 255], vec![]]).unwrap();
    assert!(store.save_corpus(&vec![vec![1]; 257]).is_err());
    assert_eq!(store.load_corpus().unwrap(), vec![vec![0, 255], vec![]]);
}

#[test]
fn structured_original_record_is_complete() {
    let root = TemporaryRoot::new("records");
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
}
#[test]
fn structured_derivative_preserves_original_record() {
    let root = TemporaryRoot::new("structured-derivative");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original(
            "tests/test_x.py::test_x[a]",
            &[0, 255],
            &serde_json::json!({"kind":"crash","signal":11}),
        )
        .unwrap();
    let bytes = fs::read(&original).unwrap();
    let derived = store
        .derived(&original, &serde_json::json!({"input":[]}))
        .unwrap();
    assert_ne!(derived, original);
    assert_eq!(fs::read(&original).unwrap(), bytes);
}
#[derive(Clone, Copy)]
enum InvalidRecord {
    Derivative,
    Raw,
    Schema,
}

fn invalid_record(store: &mut ArtifactStore, original: &str, scenario: InvalidRecord) -> String {
    match scenario {
        InvalidRecord::Derivative => store
            .derived(original, &serde_json::json!({"input":[]}))
            .unwrap(),
        InvalidRecord::Raw => store
            .save_original(b"raw")
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        InvalidRecord::Schema => {
            fs::write(
                original,
                br#"{"version":2,"test_id":"x","input":[],"observation":{}}"#,
            )
            .unwrap();
            original.into()
        }
    }
}
#[rstest::rstest]
#[case::derivative(InvalidRecord::Derivative)]
#[case::raw(InvalidRecord::Raw)]
#[case::unknown_schema(InvalidRecord::Schema)]
fn invalid_structured_derivative_sources(#[case] scenario: InvalidRecord) {
    let root = TemporaryRoot::new("invalid-record");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original("x", &[1], &serde_json::json!({}))
        .unwrap();
    let source = invalid_record(&mut store, &original, scenario);
    let before = fs::read(&source).unwrap();
    assert!(store.derived(&source, &serde_json::json!({})).is_err());
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn pending_corpus_write_preserves_prior_records() {
    let root = TemporaryRoot::new("failure");
    let mut store = ArtifactStore::open(&root).unwrap();
    store.save_corpus(&[vec![7]]).unwrap();
    fs::write(root.join("corpus.pending"), b"interrupted").unwrap();
    assert!(store.save_corpus(&[vec![8]]).is_err());
    assert_eq!(store.load_corpus().unwrap(), vec![vec![7]]);
}
#[test]
fn oversized_corpus_input_is_rejected() {
    let root = TemporaryRoot::new("large-corpus-input");
    let mut store = ArtifactStore::open(&root).unwrap();
    assert!(store.save_corpus(&[vec![0; 4097]]).is_err());
}
#[rstest::rstest]
#[case::empty_test_id("", vec![])]
#[case::oversized_input("x", vec![0;4097])]
fn invalid_original_metadata(#[case] test_id: &str, #[case] input: Vec<u8>) {
    let root = TemporaryRoot::new("invalid-metadata");
    let mut store = ArtifactStore::open(&root).unwrap();
    assert!(
        store
            .record_original(test_id, &input, &serde_json::json!({}))
            .is_err()
    );
}

#[rstest::rstest]
#[case::empty_test_id(serde_json::json!({"version":1,"test_id":"","input":[],"observation":{}}))]
#[case::invalid_byte(serde_json::json!({"version":1,"test_id":"x","input":[256],"observation":{}}))]
#[case::missing_observation(serde_json::json!({"version":1,"test_id":"x","input":[]}))]
fn incompatible_metadata_is_not_repaired(#[case] record: serde_json::Value) {
    let root = TemporaryRoot::new("metadata");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original("x[a]", &[1], &serde_json::json!({}))
        .unwrap();
    fs::write(&original, serde_json::to_vec(&record).unwrap()).unwrap();
    let before = fs::read(&original).unwrap();
    assert!(store.derived(&original, &serde_json::json!({})).is_err());
    assert_eq!(fs::read(&original).unwrap(), before);
}
#[rstest::rstest]
#[case::identity_mismatch(None)]
#[case::oversized_manifest(Some(vec![b' ';1024*1024+1]))]
fn rejected_open_preserves_records_and_releases_lock(#[case] manifest: Option<Vec<u8>>) {
    let root = TemporaryRoot::new("manifest-rejection");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original("x[a]", &[1], &serde_json::json!({}))
        .unwrap();
    drop(store);
    replace_manifest(&root, manifest);
    assert!(ArtifactStore::open_matching(&root, &serde_json::json!({})).is_err());
    assert!(std::path::Path::new(&original).exists());
    assert!(!root.join("writer.lock").exists());
}
fn replace_manifest(root: &std::path::Path, manifest: Option<Vec<u8>>) {
    if let Some(bytes) = manifest {
        fs::write(root.join("manifest.json"), bytes).unwrap();
    }
}

#[test]
fn interrupted_original_publication_is_never_visible_as_a_finding() {
    let root = TemporaryRoot::new("pending-original");
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
}
#[test]
fn publication_after_pending_collision_uses_next_name() {
    let root = TemporaryRoot::new("next-publication");
    let mut store = ArtifactStore::open(&root).unwrap();
    fs::write(root.join("finding-0.bin.pending"), b"partial write").unwrap();
    let _ = store.record_original("x", &[1], &serde_json::json!({}));
    let next = store
        .record_original("x", &[2], &serde_json::json!({}))
        .unwrap();
    assert!(std::path::Path::new(&next).exists());
}

#[test]
fn absent_corpus_loads_empty() {
    let root = TemporaryRoot::new("empty-corpus");
    let store = ArtifactStore::open(&root).unwrap();
    assert_eq!(store.load_corpus().unwrap(), [] as [Vec<u8>; 0]);
}
#[test]
fn maximum_corpus_round_trips() {
    let root = TemporaryRoot::new("maximum-corpus");
    let mut store = ArtifactStore::open(&root).unwrap();
    let maximum = vec![vec![255; 4096]; 256];
    store.save_corpus(&maximum).unwrap();
    assert_eq!(store.load_corpus().unwrap(), maximum);
}
#[rstest::rstest]
#[case::broken_json(b"broken".as_slice())]
#[case::invalid_byte(b"[[256]]".as_slice())]
fn corrupt_corpus_is_not_repaired(#[case] bytes: &[u8]) {
    let root = TemporaryRoot::new("corrupt-corpus");
    let store = ArtifactStore::open(&root).unwrap();
    fs::write(root.join("corpus.json"), bytes).unwrap();
    assert!(store.load_corpus().is_err());
    assert_eq!(fs::read(root.join("corpus.json")).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn symlink_cannot_be_used_as_an_original() {
    let root = TemporaryRoot::new("symlink");
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
}

fn replay_fixture() -> (TemporaryRoot, String, serde_json::Value) {
    let root = TemporaryRoot::new("replay-context");
    let environment = serde_json::json!({"selection":["test_x.py"],"python":"3.14"});
    let mut store = ArtifactStore::open_matching(
        &root.as_ref().join("target-0"),
        &serde_json::json!({"target":"test_x.py::test_x[a]","environment":environment}),
    )
    .unwrap();
    let original = store
        .record_original("test_x.py::test_x[a]", &[1], &serde_json::json!({}))
        .unwrap();
    drop(store);
    (root, original, environment)
}
#[rstest::rstest]
#[case::matching(true)]
#[case::mismatched(false)]
fn replay_context_checks_environment(#[case] matching: bool) {
    let (_root, original, environment) = replay_fixture();
    let context = kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).unwrap();
    assert_eq!(context.selection(), ["test_x.py"]);
    let requested = replay_environment(environment, matching);
    assert_eq!(context.check_environment(&requested).is_ok(), matching);
}
fn replay_environment(environment: serde_json::Value, matching: bool) -> serde_json::Value {
    if matching {
        environment
    } else {
        serde_json::json!({})
    }
}
#[test]
fn replay_context_excludes_campaign_writer() {
    let (root, original, _) = replay_fixture();
    let _context = kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).unwrap();
    assert!(ArtifactStore::open(root.as_ref()).is_err());
}
#[test]
fn dropping_replay_context_releases_campaign_lock() {
    let (root, original, _) = replay_fixture();
    let context = kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).unwrap();
    drop(context);
    drop(ArtifactStore::open(root.as_ref()).unwrap());
}

#[rstest::rstest]
#[case::variant_mismatch(false)]
#[case::unknown_schema(true)]
fn replay_admission_preserves_original(#[case] unknown_schema: bool) {
    let root = TemporaryRoot::new("replay-invalid");
    let target = root.join("target-0");
    let identity = serde_json::json!({"target":"test_x.py::test_x[a]","environment":{"selection":["test_x.py"]}});
    let mut store = ArtifactStore::open_matching(&target, &identity).unwrap();
    let original = store
        .record_original(
            replay_variant(unknown_schema),
            &[1, 2],
            &serde_json::json!({}),
        )
        .unwrap();
    drop(store);
    let before = fs::read(&original).unwrap();
    install_replay_schema(&target, &identity, unknown_schema);
    assert!(kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).is_err());
    assert_eq!(fs::read(original).unwrap(), before);
    drop(ArtifactStore::open(&root).unwrap());
}

fn install_replay_schema(
    target: &std::path::Path,
    identity: &serde_json::Value,
    unknown_schema: bool,
) {
    if unknown_schema {
        fs::write(
            target.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({"version":2,"identity":identity})).unwrap(),
        )
        .unwrap();
    }
}

fn replay_variant(unknown_schema: bool) -> &'static str {
    if unknown_schema {
        "test_x.py::test_x[a]"
    } else {
        "test_x.py::test_x[b]"
    }
}

#[test]
fn oversized_identity_is_rejected_before_locking() {
    let root = TemporaryRoot::new("oversized-identity");
    let identity = serde_json::json!({"value":"x".repeat(1024*1024)});
    assert!(ArtifactStore::open_matching(&root, &identity).is_err());
    assert!(!root.join("writer.lock").exists());
}

#[test]
fn oversized_derived_evidence_preserves_original() {
    let root = TemporaryRoot::new("oversized-evidence");
    let mut store = ArtifactStore::open(&root).unwrap();
    let original = store
        .record_original("x", &[1], &serde_json::json!({}))
        .unwrap();
    let before = fs::read(&original).unwrap();
    assert!(
        store
            .derived(
                &original,
                &serde_json::json!({"value":"x".repeat(1024*1024)})
            )
            .is_err()
    );
    assert_eq!(fs::read(&original).unwrap(), before);
}

#[test]
fn oversized_original_record_is_never_published() {
    let root = TemporaryRoot::new("oversized-record");
    let mut store = ArtifactStore::open(&root).unwrap();
    assert!(
        store
            .record_original(
                "x",
                &[1],
                &serde_json::json!({"value":"x".repeat(1024*1024)})
            )
            .is_err()
    );
    assert!(!root.join("finding-0.bin").exists());
}

#[test]
fn failed_corpus_rename_removes_pending_file() {
    let root = TemporaryRoot::new("corpus-rename-failure");
    let mut store = ArtifactStore::open(&root).unwrap();
    fs::create_dir(root.join("corpus.json")).unwrap();
    assert!(store.save_corpus(&[vec![1]]).is_err());
    assert!(root.join("corpus.json").is_dir());
    assert!(!root.join("corpus.pending").exists());
}

#[test]
fn replay_context_exposes_locked_record_identity() {
    let (_root, original, environment) = replay_fixture();
    let context = kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).unwrap();
    assert_eq!(context.original(), original);
    assert_eq!(
        Some(context.directory()),
        std::path::Path::new(&original).parent()
    );
    assert_eq!(
        context.store_identity(),
        &serde_json::json!({"target":"test_x.py::test_x[a]","environment":environment})
    );
}

#[test]
fn corpus_directory_is_not_treated_as_absent() {
    let root = TemporaryRoot::new("corpus-read-error");
    let store = ArtifactStore::open(&root).unwrap();
    fs::create_dir(root.join("corpus.json")).unwrap();
    assert!(store.load_corpus().is_err());
    assert!(root.join("corpus.json").is_dir());
}

#[cfg(unix)]
fn non_utf8_path(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStringExt;
    root.join(std::ffi::OsString::from_vec(vec![b'n', 255]))
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_store_rejects_structured_record_without_publication() {
    let root = TemporaryRoot::new("non-utf-store");
    let path = non_utf8_path(&root);
    let mut store = ArtifactStore::open(&path).unwrap();
    assert!(
        store
            .record_original("x", &[1], &serde_json::json!({}))
            .is_err()
    );
    assert!(!path.join("finding-0.bin").exists());
}

#[cfg(unix)]
#[test]
fn non_utf8_original_path_cannot_be_presented() {
    let root = TemporaryRoot::new("non-utf-description");
    let path = non_utf8_path(&root);
    assert!(kiryl::artifacts::describe_original(&path).is_err());
}

#[test]
fn malformed_replay_manifest_releases_campaign_lock() {
    let (root, original, _) = replay_fixture();
    fs::write(
        std::path::Path::new(&original)
            .parent()
            .unwrap()
            .join("manifest.json"),
        b"broken",
    )
    .unwrap();
    assert!(kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).is_err());
    drop(ArtifactStore::open(&root).unwrap());
}

#[test]
fn malformed_replay_record_releases_campaign_lock() {
    let (root, original, _) = replay_fixture();
    fs::write(&original, b"broken").unwrap();
    assert!(kiryl::artifacts::ReplayContext::open(std::path::Path::new(&original)).is_err());
    drop(ArtifactStore::open(&root).unwrap());
}
