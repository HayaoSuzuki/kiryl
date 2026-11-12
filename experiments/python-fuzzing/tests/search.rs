// Copyright (c) 2026 kiryl contributors
use fuzz_probe::{controller::Observation, search::Search, types::OutcomeKind};
use proptest::prelude::*;

fn observation(kind: OutcomeKind, features: &[&str]) -> Observation {
    Observation {
        kind,
        features: features.iter().map(|s| (*s).into()).collect(),
        canonical: Some(vec![4, 5]),
        ..Observation::default()
    }
}
#[test]
fn novelty_retains_valid_canonical_and_ablation_does_not() {
    let mut guided = Search::new(9);
    let mut ablated = Search::without_feedback(9);
    let input = guided.next_case();
    assert_eq!(input, ablated.next_case());
    let before = guided.corpus_len();
    let result = observation(OutcomeKind::Pass, &["branch-left"]);
    guided.observe(&input, &result);
    ablated.observe(&input, &result);
    assert_eq!(guided.corpus_len(), before + 1);
    assert_eq!(ablated.corpus_len(), before);
    let retained = guided.corpus_len();
    guided.observe(&input, &observation(OutcomeKind::Reject, &["branch-right"]));
    assert_eq!(guided.corpus_len(), retained);
    assert_eq!(guided.feature_count(), 1);
}
fn feedback_stream(seed: u64, novelty: Vec<bool>) -> Result<(), TestCaseError> {
    let mut first = Search::new(seed);
    let mut second = Search::new(seed);
    for (index, novel) in novelty.into_iter().enumerate() {
        let input = first.next_case();
        prop_assert_eq!(&input, &second.next_case());
        prop_assert!(input.len() <= 4096);
        let result = Observation {
            kind: if novel {
                OutcomeKind::Pass
            } else {
                OutcomeKind::Reject
            },
            features: vec![index.to_string()],
            ..Observation::default()
        };
        first.observe(&input, &result);
        second.observe(&input, &result);
        prop_assert!(first.corpus_len() <= 256);
        prop_assert_eq!(first.feature_count(), second.feature_count());
    }
    Ok(())
}
proptest! {
    #[test]
    fn deterministic_bounded_feedback_stream(seed in any::<u64>(), novelty in proptest::collection::vec(any::<bool>(), 0..100)) {
        feedback_stream(seed, novelty)?;
    }
}

fn model_retention_case(line: &str) -> (usize, usize) {
    let fields: Vec<_> = line.split('\t').collect();
    let mut search = Search::new(0);
    for index in 0..fields[1].parse::<u32>().unwrap() {
        let feature = if fields[0] == "duplicate" { 0 } else { index };
        let result = Observation {
            kind: if fields[0] == "reject" {
                OutcomeKind::Reject
            } else {
                OutcomeKind::Pass
            },
            canonical: Some(index.to_le_bytes().to_vec()),
            features: vec![feature.to_string()],
            ..Observation::default()
        };
        search.observe(&index.to_le_bytes(), &result);
    }
    (search.corpus_len(), search.feature_count())
}
#[test]
fn lean_retention_case_0() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .next()
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_1() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(1)
                .unwrap()
        ),
        (2, 1)
    );
}
#[test]
fn lean_retention_case_2() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(2)
                .unwrap()
        ),
        (3, 2)
    );
}
#[test]
fn lean_retention_case_3() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(3)
                .unwrap()
        ),
        (256, 255)
    );
}
#[test]
fn lean_retention_case_4() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(4)
                .unwrap()
        ),
        (256, 256)
    );
}
#[test]
fn lean_retention_case_5() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(5)
                .unwrap()
        ),
        (256, 257)
    );
}
#[test]
fn lean_retention_case_6() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(6)
                .unwrap()
        ),
        (256, 300)
    );
}
#[test]
fn lean_retention_case_7() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(7)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_8() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(8)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_9() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(9)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_10() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(10)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_11() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(11)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_12() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(12)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_13() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(13)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_14() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(14)
                .unwrap()
        ),
        (1, 0)
    );
}
#[test]
fn lean_retention_case_15() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(15)
                .unwrap()
        ),
        (2, 1)
    );
}
#[test]
fn lean_retention_case_16() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(16)
                .unwrap()
        ),
        (2, 1)
    );
}
#[test]
fn lean_retention_case_17() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(17)
                .unwrap()
        ),
        (2, 1)
    );
}
#[test]
fn lean_retention_case_18() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(18)
                .unwrap()
        ),
        (2, 1)
    );
}
#[test]
fn lean_retention_case_19() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(19)
                .unwrap()
        ),
        (2, 1)
    );
}
#[test]
fn lean_retention_case_20() {
    assert_eq!(
        model_retention_case(
            include_str!("../formal/search-cases.tsv")
                .lines()
                .nth(20)
                .unwrap()
        ),
        (2, 1)
    );
}
