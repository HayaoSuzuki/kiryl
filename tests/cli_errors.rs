mod common;
use std::{fs, process::Command};
#[test]
fn infrastructure_errors_replace_a_stale_requested_report() {
    let root = common::TemporaryRoot::new("error-report");
    let path = root.join("report.json");
    fs::write(&path, br#"{"status":"previous success"}"#).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_kiryl"))
        .args(["unknown-command", "--report"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "infrastructure");
    assert!(
        report["infrastructure_errors"][0]
            .as_str()
            .unwrap()
            .contains("unknown command")
    );
    fs::remove_file(path).unwrap();
}

#[rstest::rstest]
#[case::unknown_option(vec!["run", "--unknown", "x"], "unknown option")]
#[case::missing_value(vec!["run", "--python"], "missing option value")]
#[case::memory_overflow(vec!["run", "--memory-mib", "18446744073709551615"], "memory overflow")]
#[case::zero_memory(vec!["run", "--memory-mib", "0"], "memory must be positive")]
#[case::too_much_output(vec!["run", "--output-bytes", "16777217"], "retained output cannot exceed")]
#[case::two_findings(vec!["replay", "first", "second"], "expected one finding")]
#[case::missing_replay_finding(vec!["replay"], "missing finding")]
#[case::missing_minimize_finding(vec!["minimize"], "missing finding")]
fn invalid_arguments_return_structured_error(
    #[case] arguments: Vec<&str>,
    #[case] diagnostic: &str,
) {
    let result = Command::new(env!("CARGO_BIN_EXE_kiryl"))
        .args(arguments)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        report["infrastructure_errors"][0]
            .as_str()
            .unwrap()
            .contains(diagnostic)
    );
}

#[rstest::rstest]
#[case::default(vec![])]
#[case::long(vec!["--help"])]
#[case::short(vec!["-h"])]
#[case::named(vec!["help"])]
fn help_succeeds(#[case] arguments: Vec<&str>) {
    let result = Command::new(env!("CARGO_BIN_EXE_kiryl"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("kiryl minimize")
    );
}

#[test]
fn error_report_write_failure_preserves_diagnostic_on_stdout() {
    let root = common::TemporaryRoot::new("error-report-unwritable");
    let result = Command::new(env!("CARGO_BIN_EXE_kiryl"))
        .args(["unknown-command", "--report"])
        .arg(root.join("missing/report.json"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["status"], "infrastructure");
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("error report persistence failed")
    );
}
