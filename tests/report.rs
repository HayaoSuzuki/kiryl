use serde_json::json;
#[test]
fn finding_reports_present_values_and_reproduction_commands() {
    let root = std::env::temp_dir().join(format!("kiryl-report-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("finding-0.bin");
    std::fs::write(
        &path,
        br#"{"version":1,"test_id":"test_x","input":[],"observation":{"rendered":"x=7","fingerprint":"assertion","replay_token":[1]}}"#,
    )
    .unwrap();
    let mut report = json!({"outcomes":[{"original":path}]});
    kiryl::report::present_findings(
        &mut report,
        std::path::Path::new("kiryl"),
        std::path::Path::new("python"),
        true,
    )
    .unwrap();
    assert_eq!(report["outcomes"][0]["input_values"], "x=7");
    assert_eq!(report["outcomes"][0]["replay_command"][1], "replay");
    assert_eq!(report["outcomes"][0]["minimize_command"][1], "minimize");
    assert_eq!(report["outcomes"][0]["replay_command"][5], "--best-effort");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn standalone_and_campaign_use_the_same_exit_rules() {
    for (kind, expected) in [("pass", 0), ("failure", 1), ("infrastructure", 2)] {
        let standalone = json!({"outcomes":[{"kind":kind}]});
        let campaign = json!({"targets":[{"report":standalone}]});
        assert_eq!(kiryl::report::exit_code(&standalone), expected);
        assert_eq!(kiryl::report::exit_code(&campaign), expected);
    }
    assert_eq!(kiryl::report::exit_code(&json!({"interrupted":true})), 130);
}

#[test]
fn cancellation_preserves_candidates_and_interruption_dominates_errors() {
    let mut report = json!({"outcomes":[{"kind":"campaign_cancelled","original":null}]});
    assert_eq!(kiryl::report::exit_code(&report), 0);
    report["outcomes"][0]["original"] = json!("finding-0.bin");
    assert_eq!(kiryl::report::exit_code(&report), 1);
    report["outcomes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"infrastructure"}));
    assert_eq!(kiryl::report::exit_code(&report), 2);
    report["interrupted"] = json!(true);
    assert_eq!(kiryl::report::exit_code(&report), 130);
}

#[test]
fn command_errors_share_completion_and_preserve_available_context() {
    use std::{io, time::Duration};
    for (kind, expected) in [
        (io::ErrorKind::Interrupted, 130),
        (io::ErrorKind::TimedOut, 0),
        (io::ErrorKind::Other, 2),
    ] {
        let mut report = json!({"command":"run","selection":["test_x.py"]});
        let completion = kiryl::report::record_error(
            &mut report,
            &io::Error::new(kind, "reason"),
            Duration::from_millis(7),
        );
        assert_eq!(completion.exit_code(), expected);
        assert_eq!(report["selection"][0], "test_x.py");
        assert_eq!(report["elapsed_ms"], 7);
    }
}

#[test]
fn publication_replaces_complete_reports_and_preserves_them_on_pending_collision() {
    let root = std::env::temp_dir().join(format!("kiryl-publication-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("report.json");
    std::fs::write(&path, b"stale").unwrap();
    kiryl::report::publish_output(Some(&path), &json!({"complete":true})).unwrap();
    let prior = std::fs::read(&path).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prior).unwrap(),
        json!({"complete":true})
    );
    let pending = path.with_extension(format!("pending-{}", std::process::id()));
    std::fs::write(&pending, b"another writer").unwrap();
    assert!(kiryl::report::publish_output(Some(&path), &json!({"complete":false})).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), prior);
    assert_eq!(std::fs::read(&pending).unwrap(), b"another writer");
    std::fs::remove_dir_all(root).unwrap();
}
