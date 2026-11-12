use fuzz_probe::types::{OutcomeKind, Verification};
use proptest::prelude::*;

#[test]
fn failure_has_literal_wire_label() {
    assert_eq!(
        serde_json::to_string(&OutcomeKind::Failure).unwrap(),
        "\"failure\""
    );
}
#[test]
fn missing_observation_state_preserves_empty_label() {
    let observation: fuzz_probe::controller::Observation = serde_json::from_str("{}").unwrap();
    assert_eq!(serde_json::to_value(observation).unwrap()["kind"], "");
}
#[test]
fn verification_diagnostic_preserves_literal_text() {
    assert_eq!(
        serde_json::to_string(&Verification::Diagnostic("cleanup failed: x".into())).unwrap(),
        "\"cleanup failed: x\""
    );
}
proptest! {
    #[test]
    fn arbitrary_outcome_labels_round_trip(label in any::<String>()) {
        let encoded = serde_json::to_value(&label).unwrap();
        let kind: OutcomeKind = serde_json::from_value(encoded.clone()).unwrap();
        prop_assert_eq!(serde_json::to_value(kind).unwrap(), encoded);
    }
}

#[test]
fn unknown_label_remains_explicit_other() {
    let kind: OutcomeKind = serde_json::from_str("\"future-outcome\"").unwrap();
    assert_eq!(kind, OutcomeKind::Other("future-outcome".into()));
}

proptest! {
    #[test]
    fn arbitrary_verification_labels_round_trip(label in any::<String>()) {
        let encoded = serde_json::to_value(&label).unwrap();
        let verification: Verification = serde_json::from_value(encoded.clone()).unwrap();
        prop_assert_eq!(serde_json::to_value(verification).unwrap(), encoded);
    }
}
