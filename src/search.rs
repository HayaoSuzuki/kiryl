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

    #[test]
    fn novel_pass_uses_canonical_bytes_and_failure_keeps_original_bytes() {
        let mut search = Search::new(7);
        assert_eq!(search.next_case(), vec![0; 512]);
        let mut observation = Observation {
            kind: "pass".into(),
            canonical: Some(vec![1, 2]),
            features: vec!["branch-a".into()],
            ..Observation::default()
        };
        search.observe(&[9], &observation);
        assert_eq!(search.corpus(), [vec![0; 512], vec![1, 2]]);
        observation.kind = "failure".into();
        observation.features = vec!["branch-b".into()];
        search.observe(&[3, 4], &observation);
        assert_eq!(search.corpus(), [vec![0; 512], vec![1, 2], vec![3, 4]]);
        observation.kind = "reject".into();
        observation.features = vec!["ignored".into()];
        search.observe(&[5], &observation);
        assert_eq!(search.feature_count(), 2);
        assert_eq!(search.corpus_len(), 3);
    }
}
