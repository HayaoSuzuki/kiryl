// Copyright (c) 2026 kiryl contributors
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let Some((&first, input)) = bytes.split_first() else {
        return;
    };
    let limit = usize::from(first);
    let mut tail = fuzz_probe::controller::LogTail::default();
    for chunk in input.chunks(31) {
        tail.append(chunk, limit);
    }
    let expected = &input[input.len().saturating_sub(limit)..];
    assert_eq!(tail.tail.into_iter().collect::<Vec<_>>(), expected);
    assert_eq!(tail.observed, input.len() as u64);
});
