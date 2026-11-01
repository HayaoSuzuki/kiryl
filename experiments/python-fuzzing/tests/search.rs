// Copyright (c) 2026 kiryl contributors
use fuzz_probe::{controller::Observation, search::Search};
use proptest::prelude::*;

fn observation(kind: &str, features: &[&str]) -> Observation {
    Observation {
        kind: kind.into(),
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
    let result = observation("pass", &["branch-left"]);
    guided.observe(&input, &result);
    ablated.observe(&input, &result);
    assert_eq!(guided.corpus_len(), before + 1);
    assert_eq!(ablated.corpus_len(), before);
    let retained = guided.corpus_len();
    guided.observe(&input, &observation("reject", &["branch-right"]));
    assert_eq!(guided.corpus_len(), retained);
    assert_eq!(guided.feature_count(), 1);
}
proptest! {
    #[test]
    fn deterministic_bounded_feedback_stream(seed in any::<u64>(), novelty in proptest::collection::vec(any::<bool>(), 0..100)) {
        let mut first = Search::new(seed);
        let mut second = Search::new(seed);
        for (index, novel) in novelty.into_iter().enumerate() {
            let input = first.next_case();
            prop_assert_eq!(&input, &second.next_case());
            prop_assert!(input.len() <= 4096);
            let result = Observation { kind: if novel { "pass" } else { "reject" }.into(), features: vec![index.to_string()], ..Observation::default() };
            first.observe(&input, &result);
            second.observe(&input, &result);
            prop_assert!(first.corpus_len() <= 256);
            prop_assert_eq!(first.feature_count(), second.feature_count());
        }
    }
}

#[test]
fn lean_generated_retention_count_correspondence() {
    // internal-fixture: unique four-byte canonical payloads are distinct from
    // the zero seed. Model counts do not specify mutation/strategy semantics.
    for line in include_str!("../formal/search-cases.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let mut search = Search::new(0);
        for index in 0..fields[1].parse::<u32>().unwrap() {
            let feature = if fields[0] == "duplicate" { 0 } else { index };
            let result = Observation {
                kind: if fields[0] == "reject" {
                    "reject"
                } else {
                    "pass"
                }
                .into(),
                canonical: Some(index.to_le_bytes().to_vec()),
                features: vec![feature.to_string()],
                ..Observation::default()
            };
            search.observe(&index.to_le_bytes(), &result);
        }
        assert_eq!(search.corpus_len(), fields[2].parse::<usize>().unwrap());
        assert_eq!(search.feature_count(), fields[3].parse::<usize>().unwrap());
    }
}
