#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    // Equal-length slices are the campaign caller's invariant; cap allocation.
    let entries: Vec<_> = bytes.chunks_exact(17).take(256).collect();
    let charges: Vec<u128> = entries
        .iter()
        .map(|entry| u128::from_le_bytes(entry[..16].try_into().unwrap()))
        .collect();
    let stopped: Vec<bool> = entries.iter().map(|entry| entry[16] & 1 != 0).collect();
    let actual = kiryl::campaign::choose_next(&charges, &stopped);
    match actual {
        Some(index) => {
            assert!(!stopped[index]);
            for i in 0..charges.len() {
                if !stopped[i] {
                    assert!(charges[index] <= charges[i]);
                    if charges[index] == charges[i] {
                        assert!(index <= i);
                    }
                }
            }
        }
        None => assert!(stopped.iter().all(|stop| *stop)),
    }
});
