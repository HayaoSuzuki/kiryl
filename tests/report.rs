use serde_json::json;
#[test]
fn finding_reports_present_values_and_reproduction_commands() {
    let root = std::env::temp_dir().join(format!("kiryl-report-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("finding.bin");
    std::fs::write(
        &path,
        br#"{"observation":{"rendered":"x=7","fingerprint":"assertion","replay_token":[1]}}"#,
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
