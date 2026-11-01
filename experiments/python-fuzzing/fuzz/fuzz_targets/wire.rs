// Copyright (c) 2026 kiryl contributors
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let _ = fuzz_probe::wire::decode(bytes);
    let mut cursor = bytes;
    let _ = fuzz_probe::wire::read(&mut cursor);
});
