use kiryl::campaign::choose_next;
#[test]
fn least_charged_unfinished_target_gets_next_slice() {
    assert_eq!(choose_next(&[40, 0, 20], &[false, false, false]), Some(1));
    assert_eq!(choose_next(&[40, 0, 20], &[false, true, false]), Some(2));
    assert_eq!(choose_next(&[40], &[true]), None);
}

#[test]
fn missing_target_state_is_not_admitted() {
    assert_eq!(choose_next(&[0, 1], &[true]), None);
}

#[test]
fn pending_active_cancellation_is_not_success() {
    let report = serde_json::json!({"targets":[{"report":{"outcomes":[{"kind":"campaign_cancelled","original":"finding-0.bin"}]}}]});
    assert_eq!(kiryl::report::exit_code(&report), 1);
}
