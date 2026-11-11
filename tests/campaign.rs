use kiryl::campaign::choose_next;
#[test]
fn least_charged_unfinished_target_gets_next_slice() {
    assert_eq!(choose_next(&[40, 0, 20], &[false, false, false]), Some(1));
    assert_eq!(choose_next(&[40, 0, 20], &[false, true, false]), Some(2));
    assert_eq!(choose_next(&[40], &[true]), None);
}
