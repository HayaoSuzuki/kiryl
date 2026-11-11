// Copyright (c) 2026 kiryl contributors
//! Deterministic bounded byte mutation; Hypothesis strategies stay Python-owned.
use crate::executor::Observation;
use std::collections::HashSet;

const MAX_INPUT: usize = 4096;
const MAX_CORPUS: usize = 256;
const MAX_FEATURES: usize = 100_000;

pub struct Search {
    random: u64,
    corpus: Vec<Vec<u8>>,
    features: HashSet<String>,
    feature_bytes: usize,
    first: bool,
    pub feature_limit_reached: bool,
}
impl Search {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            random: seed,
            corpus: vec![vec![0; 512]],
            features: HashSet::new(),
            feature_bytes: 0,
            first: true,
            feature_limit_reached: false,
        }
    }
    pub fn restore(&mut self, corpus: Vec<Vec<u8>>) {
        if !corpus.is_empty() {
            self.corpus = corpus;
        }
    }
    pub fn corpus(&self) -> &[Vec<u8>] {
        &self.corpus
    }
    fn random(&mut self) -> u64 {
        // SplitMix64: wrapping arithmetic makes the stream independent of builds.
        self.random = self.random.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.random;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
    fn index(&mut self, bound: usize) -> usize {
        // Bounds are small fixed constants or bounded vector lengths.
        usize::try_from(self.random() % u64::try_from(bound).unwrap_or(u64::MAX)).unwrap_or(0)
    }
    fn byte(&mut self) -> u8 {
        self.random().to_le_bytes()[0]
    }
    pub fn next_case(&mut self) -> Vec<u8> {
        if self.first {
            self.first = false;
            return self.corpus[0].clone();
        }
        if self.index(8) == 0 {
            let len = self.index(1024) + 1;
            return (0..len).map(|_| self.byte()).collect();
        }
        let index = self.index(self.corpus.len());
        let mut candidate = self.corpus[index].clone();
        match self.index(4) {
            0 if !candidate.is_empty() => {
                let index = self.index(candidate.len());
                candidate[index] = self.byte();
            }
            1 if !candidate.is_empty() => {
                let index = self.index(candidate.len());
                candidate[index] ^= 1 << self.index(8);
            }
            2 if !candidate.is_empty() => {
                let index = self.index(candidate.len());
                candidate.remove(index);
            }
            _ if candidate.len() < MAX_INPUT => {
                let index = self.index(candidate.len() + 1);
                candidate.insert(index, self.byte());
            }
            _ => (),
        }
        candidate
    }
    pub fn observe(&mut self, input: &[u8], observation: &Observation) {
        if !["pass", "failure"].contains(&observation.kind.as_str()) {
            return;
        }
        let mut novel = false;
        for feature in &observation.features {
            if self.features.contains(feature) {
                continue;
            }
            if self.features.len() >= MAX_FEATURES
                || feature.len() > 4096
                || self.feature_bytes.saturating_add(feature.len()) > 16 * 1024 * 1024
            {
                self.feature_limit_reached = true;
                break;
            }
            self.feature_bytes += feature.len();
            novel |= self.features.insert(feature.clone());
        }
        if novel {
            let candidate = if observation.kind == "pass" {
                observation.canonical.as_deref().unwrap_or(input)
            } else {
                input
            };
            if candidate.len() <= MAX_INPUT && !self.corpus.iter().any(|entry| entry == candidate) {
                if self.corpus.len() < MAX_CORPUS {
                    self.corpus.push(candidate.to_vec());
                } else {
                    let index = self.index(MAX_CORPUS);
                    self.corpus[index] = candidate.to_vec();
                }
            }
        }
    }
    #[must_use]
    pub fn corpus_len(&self) -> usize {
        self.corpus.len()
    }
    #[must_use]
    pub fn feature_count(&self) -> usize {
        self.features.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn initial_case_is_zero_seed() {
        assert_eq!(Search::new(7).next_case(), vec![0; 512]);
    }
    #[rstest::rstest]
    #[case("pass", vec![1, 2])]
    #[case("failure", vec![9])]
    fn novel_observation_selects_candidate_bytes(#[case] kind: &str, #[case] expected: Vec<u8>) {
        let mut search = Search::new(7);
        let observation = Observation {
            kind: kind.into(),
            canonical: Some(vec![1, 2]),
            features: vec!["branch-a".into()],
            ..Observation::default()
        };
        search.observe(&[9], &observation);
        assert_eq!(search.corpus(), [vec![0; 512], expected]);
        assert_eq!(search.feature_count(), 1);
    }
    #[test]
    fn rejected_observation_does_not_change_corpus_or_features() {
        let mut search = Search::new(7);
        let observation = Observation {
            kind: "reject".into(),
            features: vec!["ignored".into()],
            ..Observation::default()
        };
        search.observe(&[5], &observation);
        assert_eq!(search.feature_count(), 0);
        assert_eq!(search.corpus(), [vec![0; 512]]);
    }
    #[rstest::rstest]
    #[case(vec![], vec![vec![0; 512]])]
    #[case(vec![vec![1, 2]], vec![vec![1, 2]])]
    fn restore_preserves_nonempty_corpus(
        #[case] corpus: Vec<Vec<u8>>,
        #[case] expected: Vec<Vec<u8>>,
    ) {
        let mut search = Search::new(0);
        search.restore(corpus);
        assert_eq!(search.corpus(), expected);
        assert_eq!(search.corpus_len(), expected.len());
    }
    #[test]
    fn insertion_mutation_at_input_limit_preserves_candidate() {
        let mut search = Search::new(0);
        search.restore(vec![vec![1; MAX_INPUT]]);
        search.first = false;
        assert_eq!(search.next_case(), vec![1; MAX_INPUT]);
    }
    #[rstest::rstest]
    #[case(MAX_FEATURES, 0, "new".into())]
    #[case(0, 0, "x".repeat(4097))]
    #[case(0, 16 * 1024 * 1024, "new".into())]
    fn exhausted_feature_budget_sets_limit_and_keeps_corpus(
        #[case] count: usize,
        #[case] bytes: usize,
        #[case] feature: String,
    ) {
        let mut search = Search::new(0);
        search.features = (0..count).map(|index| index.to_string()).collect();
        search.feature_bytes = bytes;
        let observation = Observation {
            kind: "pass".into(),
            features: vec![feature],
            ..Observation::default()
        };
        search.observe(&[1], &observation);
        assert!(search.feature_limit_reached);
        assert_eq!(search.feature_count(), count);
        assert_eq!(search.corpus(), [vec![0; 512]]);
    }
    #[test]
    fn novel_feature_replaces_entry_when_corpus_is_full() {
        let mut search = Search::new(0);
        search.restore(vec![vec![0]; MAX_CORPUS]);
        let observation = Observation {
            kind: "failure".into(),
            features: vec!["new".into()],
            ..Observation::default()
        };
        search.observe(&[1], &observation);
        assert_eq!(search.corpus_len(), MAX_CORPUS);
        assert_eq!(
            search
                .corpus()
                .iter()
                .filter(|entry| entry.as_slice() == [1])
                .count(),
            1
        );
        assert_eq!(search.feature_count(), 1);
    }
    #[rstest::rstest]
    fn novel_feature_without_admissible_candidate_preserves_corpus(
        #[values(512, MAX_INPUT + 1)] length: usize,
    ) {
        let mut search = Search::new(0);
        let observation = Observation {
            kind: "pass".into(),
            features: vec!["new".into()],
            ..Observation::default()
        };
        search.observe(&vec![0; length], &observation);
        assert_eq!(search.feature_count(), 1);
        assert_eq!(search.corpus(), [vec![0; 512]]);
    }
    proptest! {
        #[test]
        fn observing_same_feature_twice_is_idempotent(input in proptest::collection::vec(any::<u8>(), 0..4097), feature in "[a-z]{1,32}") {
            let mut search = Search::new(7);
            let observation = Observation { kind: "pass".into(), features: vec![feature], ..Observation::default() };
            search.observe(&input, &observation);
            let corpus = search.corpus().to_vec();
            search.observe(&input, &observation);
            prop_assert_eq!(search.corpus(), corpus);
            prop_assert_eq!(search.feature_count(), 1);
        }
        #[test]
        fn mutation_stream_is_deterministic_and_bounded(seed in any::<u64>(), length in 0usize..128) {
            let mut left = Search::new(seed);
            let mut right = Search::new(seed);
            let left_cases: Vec<_> = (0..length).map(|_| left.next_case()).collect();
            let right_cases: Vec<_> = (0..length).map(|_| right.next_case()).collect();
            prop_assert_eq!(&left_cases, &right_cases);
            prop_assert!(left_cases.iter().all(|case| case.len() <= MAX_INPUT));
        }
    }
}
