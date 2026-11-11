mod common;
use proptest::prelude::*;
use serde_json::json;
#[test]
fn finding_reports_present_values_and_reproduction_commands() {
    let root = common::TemporaryRoot::new("report");
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
}

#[rstest::rstest]
#[case::standalone_pass(json!({"outcomes":[{"kind":"pass"}]}), 0)]
#[case::standalone_failure(json!({"outcomes":[{"kind":"failure"}]}), 1)]
#[case::standalone_infrastructure(json!({"outcomes":[{"kind":"infrastructure"}]}), 2)]
#[case::campaign_pass(json!({"targets":[{"report":{"outcomes":[{"kind":"pass"}]}}]}), 0)]
#[case::campaign_failure(json!({"targets":[{"report":{"outcomes":[{"kind":"failure"}]}}]}), 1)]
#[case::campaign_infrastructure(json!({"targets":[{"report":{"outcomes":[{"kind":"infrastructure"}]}}]}), 2)]
#[case::interrupted(json!({"interrupted":true}), 130)]
#[case::cancelled_without_candidate(json!({"outcomes":[{"kind":"campaign_cancelled","original":null}]}), 0)]
#[case::cancelled_with_candidate(json!({"outcomes":[{"kind":"campaign_cancelled","original":"finding-0.bin"}]}), 1)]
#[case::infrastructure_dominates_candidate(json!({"outcomes":[{"kind":"campaign_cancelled","original":"finding-0.bin"},{"kind":"infrastructure"}]}), 2)]
#[case::interruption_dominates_errors(json!({"interrupted":true,"outcomes":[{"kind":"campaign_cancelled","original":"finding-0.bin"},{"kind":"infrastructure"}]}), 130)]
fn reports_follow_exit_precedence(#[case] report: serde_json::Value, #[case] expected: i32) {
    assert_eq!(kiryl::report::exit_code(&report), expected);
}

#[rstest::rstest]
#[case::interrupted(std::io::ErrorKind::Interrupted, 130)]
#[case::timed_out(std::io::ErrorKind::TimedOut, 0)]
#[case::other(std::io::ErrorKind::Other, 2)]
fn command_errors_preserve_context(#[case] kind: std::io::ErrorKind, #[case] expected: i32) {
    let mut report = json!({"command":"run","selection":["test_x.py"]});
    let completion = kiryl::report::record_error(
        &mut report,
        &std::io::Error::new(kind, "reason"),
        std::time::Duration::from_millis(7),
    );
    assert_eq!(completion.exit_code(), expected);
    assert_eq!(report["selection"][0], "test_x.py");
    assert_eq!(report["elapsed_ms"], 7);
}

proptest! {
    #[test]
    fn report_order_and_campaign_nesting_preserve_severity(kinds in prop::collection::vec(prop_oneof![Just("pass"), Just("failure"), Just("infrastructure"), Just("skip"), Just("campaign_cancelled")], 0..40)) {
        let outcomes: Vec<_> = kinds.iter().map(|kind| json!({"kind":kind})).collect();
        let report = json!({"outcomes":outcomes});
        let reversed: Vec<_> = outcomes.iter().rev().cloned().collect();
        let nested = json!({"targets":[{"report":{"outcomes":reversed}}]});
        prop_assert_eq!(kiryl::report::exit_code(&report), kiryl::report::exit_code(&nested));
    }
    #[test]
    fn interruption_dominates_arbitrary_existing_outcomes(kinds in prop::collection::vec("[a-z_]{0,30}", 0..40)) {
        let outcomes: Vec<_> = kinds.iter().map(|kind| json!({"kind":kind})).collect();
        let report = json!({"targets":[{"report":{"outcomes":outcomes}},{"report":{"interrupted":true}}]});
        prop_assert_eq!(kiryl::report::exit_code(&report), 130);
    }
}

#[test]
fn publication_replaces_complete_reports() {
    let root = common::TemporaryRoot::new("publication");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("report.json");
    std::fs::write(&path, b"stale").unwrap();
    kiryl::report::publish_output(Some(&path), &json!({"complete":true})).unwrap();
    let prior = std::fs::read(&path).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prior).unwrap(),
        json!({"complete":true})
    );
}

#[test]
fn pending_collision_preserves_published_report_and_other_writer() {
    let root = common::TemporaryRoot::new("publication-collision");
    let path = root.join("report.json");
    let prior = br#"{"complete":true}"#;
    std::fs::write(&path, prior).unwrap();
    let pending = path.with_extension(format!("pending-{}", std::process::id()));
    std::fs::write(&pending, b"another writer").unwrap();
    assert!(kiryl::report::publish_output(Some(&path), &json!({"complete":false})).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), prior);
    assert_eq!(std::fs::read(&pending).unwrap(), b"another writer");
}

#[test]
fn campaign_raw_finding_replay_uses_effective_limits_without_minimization() {
    let root = common::TemporaryRoot::new("raw-report");
    let path = root.join("finding-0.bin");
    std::fs::write(&path, br#"{"version":1,"test_id":"test_x","input":[],"observation":{"rendered":null,"fingerprint":null,"replay_token":null}}"#).unwrap();
    let mut report = json!({"effective_limits":{"tree_memory_bytes":2_097_152,"case_ms":1250},"targets":[{"report":{"outcomes":[{"original":path},{"kind":"pass"}]}}]});
    kiryl::report::present_findings(
        &mut report,
        std::path::Path::new("kiryl"),
        std::path::Path::new("python"),
        false,
    )
    .unwrap();
    let finding = &report["targets"][0]["report"]["outcomes"][0];
    assert_eq!(
        finding["replay_command"],
        json!([
            "kiryl",
            "replay",
            path,
            "--python",
            "python",
            "--memory-mib",
            "2",
            "--case-timeout",
            "1.25"
        ])
    );
    assert!(finding.get("minimize_command").is_none());
    assert_eq!(
        report["targets"][0]["report"]["outcomes"][1],
        json!({"kind":"pass"})
    );
}

#[test]
fn failed_report_rename_preserves_destination_and_removes_pending_file() {
    let root = common::TemporaryRoot::new("report-rename-failure");
    let path = root.join("destination");
    std::fs::create_dir(&path).unwrap();
    assert!(kiryl::report::publish_output(Some(&path), &json!({"complete":true})).is_err());
    assert!(path.is_dir());
    assert!(
        !path
            .with_extension(format!("pending-{}", std::process::id()))
            .exists()
    );
}

#[test]
fn campaign_finding_presentation_propagates_malformed_record() {
    let root = common::TemporaryRoot::new("bad-campaign-record");
    let path = root.join("finding-0.bin");
    std::fs::write(&path, b"broken").unwrap();
    let mut report = json!({"targets":[{"report":{"outcomes":[{"original":path}]}}]});
    assert!(
        kiryl::report::present_findings(
            &mut report,
            std::path::Path::new("kiryl"),
            std::path::Path::new("python"),
            false
        )
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"broken");
}

#[test]
fn infrastructure_status_without_outcomes_is_an_error() {
    assert_eq!(
        kiryl::report::exit_code(&json!({"status":"infrastructure"})),
        2
    );
}
