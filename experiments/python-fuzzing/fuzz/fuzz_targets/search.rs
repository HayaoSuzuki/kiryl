// Copyright (c) 2026 kiryl contributors
#![no_main]
use fuzz_probe::{controller::Observation, search::Search, types::OutcomeKind};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let mut search = Search::new(0);
    for chunk in bytes.chunks(17).take(256) {
        let input = search.next_case();
        assert!(input.len() <= 4096);
        let prior = search.corpus_len();
        let valid = chunk.first().is_some_and(|b| b & 1 == 1);
        let observation = Observation {
            kind: if valid { OutcomeKind::Pass } else { OutcomeKind::Reject },
            canonical: Some(chunk.to_vec()),
            features: vec![format!("{chunk:?}")],
            ..Observation::default()
        };
        search.observe(&input, &observation);
        assert!(search.corpus_len() <= 256);
        if !valid {
            assert_eq!(prior, search.corpus_len());
        }
    }
});
