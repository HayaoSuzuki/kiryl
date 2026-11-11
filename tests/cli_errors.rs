use std::{fs, process::Command};
#[test]
fn infrastructure_errors_replace_a_stale_requested_report() {
    let path = std::env::temp_dir().join(format!("kiryl-error-report-{}.json", std::process::id()));
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
