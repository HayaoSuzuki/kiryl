use kiryl::campaign::choose_next;
#[rstest::rstest]
#[case::least_charged(vec![40, 0, 20], vec![false, false, false], Some(1))]
#[case::skip_finished(vec![40, 0, 20], vec![false, true, false], Some(2))]
#[case::all_finished(vec![40], vec![true], None)]
fn least_charged_unfinished_target_gets_next_slice(
    #[case] charged: Vec<u128>,
    #[case] finished: Vec<bool>,
    #[case] expected: Option<usize>,
) {
    assert_eq!(choose_next(&charged, &finished), expected);
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
