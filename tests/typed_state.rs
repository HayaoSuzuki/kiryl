use kiryl::executor::{Observation, OutcomeKind};

#[rstest::rstest]
#[case("pass", OutcomeKind::Pass)]
#[case("failure", OutcomeKind::Failure)]
#[case("reject", OutcomeKind::Reject)]
#[case("skip", OutcomeKind::Skip)]
#[case("oom", OutcomeKind::Oom)]
#[case("native_crash", OutcomeKind::NativeCrash)]
#[case("case_timeout", OutcomeKind::CaseTimeout)]
#[case("campaign_cancelled", OutcomeKind::CampaignCancelled)]
#[case("interrupted", OutcomeKind::Interrupted)]
#[case("infrastructure", OutcomeKind::Infrastructure)]
#[case("future-result", OutcomeKind::Other("future-result".into()))]
#[case("", OutcomeKind::Other(String::new()))]
fn observation_kind_preserves_wire_labels(#[case] label: &str, #[case] expected: OutcomeKind) {
    let observation: Observation =
        serde_json::from_value(serde_json::json!({"kind":label})).unwrap();
    assert_eq!(observation.kind, expected);
    assert_eq!(serde_json::to_value(observation).unwrap()["kind"], label);
}

#[rstest::rstest]
#[case(serde_json::json!(null))]
#[case(serde_json::json!(17))]
#[case(serde_json::json!({}))]
fn observation_kind_rejects_non_string_labels(#[case] label: serde_json::Value) {
    assert!(serde_json::from_value::<Observation>(serde_json::json!({"kind":label})).is_err());
}

#[rstest::rstest]
#[case("", kiryl::verification::Verification::Empty)]
#[case(
    "pending",
    kiryl::verification::Verification::Verdict(kiryl::verification::Verdict::Pending)
)]
#[case(
    "confirmed",
    kiryl::verification::Verification::Verdict(kiryl::verification::Verdict::Confirmed)
)]
#[case(
    "unstable",
    kiryl::verification::Verification::Verdict(kiryl::verification::Verdict::Unstable)
)]
#[case(
    "skipped",
    kiryl::verification::Verification::Verdict(kiryl::verification::Verdict::Skipped)
)]
#[case(
    "rejected",
    kiryl::verification::Verification::Verdict(kiryl::verification::Verdict::Rejected)
)]
#[case("cleanup failed: disk full", kiryl::verification::Verification::Diagnostic("cleanup failed: disk full".into()))]
fn verification_preserves_state_and_diagnostic_strings(
    #[case] label: &str,
    #[case] expected: kiryl::verification::Verification,
) {
    let observation: Observation =
        serde_json::from_value(serde_json::json!({"verification":label})).unwrap();
    assert_eq!(observation.verification, expected);
    assert_eq!(
        serde_json::to_value(observation).unwrap()["verification"],
        label
    );
}

#[rstest::rstest]
#[case(kiryl::executor::ExecutionMode::Empty, "")]
#[case(kiryl::executor::ExecutionMode::Guided, "guided")]
#[case(
    kiryl::executor::ExecutionMode::Reproduction(kiryl::executor::Reproduction::Replay),
    "replay"
)]
#[case(
    kiryl::executor::ExecutionMode::Reproduction(kiryl::executor::Reproduction::Minimize),
    "minimize"
)]
fn execution_mode_has_unchanged_wire_label(
    #[case] mode: kiryl::executor::ExecutionMode,
    #[case] label: &str,
) {
    assert_eq!(serde_json::to_value(mode).unwrap(), label);
}

#[rstest::rstest]
#[case(serde_json::json!(null))]
#[case(serde_json::json!(17))]
#[case(serde_json::json!({}))]
fn verification_rejects_non_string_evidence(#[case] label: serde_json::Value) {
    assert!(
        serde_json::from_value::<Observation>(serde_json::json!({"verification":label})).is_err()
    );
}

#[test]
fn omitted_evidence_retains_empty_version_one_defaults() {
    let observation: Observation = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(observation.kind, OutcomeKind::Other(String::new()));
    assert_eq!(
        observation.verification,
        kiryl::verification::Verification::Empty
    );
    let encoded = serde_json::to_value(observation).unwrap();
    assert_eq!(encoded["kind"], "");
    assert_eq!(encoded["verification"], "");
}

proptest::proptest! {
    #[test]
    fn arbitrary_outcome_labels_roundtrip(label in ".{0,120}") {
        let observation: Observation = serde_json::from_value(serde_json::json!({"kind":label})).unwrap();
        proptest::prop_assert_eq!(&serde_json::to_value(observation).unwrap()["kind"], &label);
    }
    #[test]
    fn arbitrary_verification_text_roundtrips(label in ".{0,120}") {
        let observation: Observation = serde_json::from_value(serde_json::json!({"verification":label})).unwrap();
        proptest::prop_assert_eq!(&serde_json::to_value(observation).unwrap()["verification"], &label);
    }
}
