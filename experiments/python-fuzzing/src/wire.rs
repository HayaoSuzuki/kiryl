// Copyright (c) 2026 kiryl contributors
//! Bounded framing independent of arbitrary target output.
use std::io::{self, Read, Write};

use serde_json::Value;

pub const MAX_FRAME: usize = 8 * 1024 * 1024;

/// Read a versioned bounded object. EOF before a header is clean stream closure.
///
/// # Errors
/// Returns invalid-data errors for incompatible or oversized frames and I/O errors
/// for partial frames. The caller must treat every such error as infrastructure.
pub fn read(reader: &mut impl Read) -> io::Result<Option<Value>> {
    let mut header = [0; 4];
    match reader.read(&mut header[..1])? {
        0 => return Ok(None),
        _ => reader.read_exact(&mut header[1..])?,
    }
    let size = u32::from_be_bytes(header) as usize;
    if size > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized frame",
        ));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    decode(&bytes).map(Some)
}

/// Decode a single payload, independently usable by fuzz targets.
///
/// # Errors
/// Rejects malformed JSON, non-objects, and unknown/inexact versions.
pub fn decode(bytes: &[u8]) -> io::Result<Value> {
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized frame",
        ));
    }
    let value: Value = serde_json::from_slice(bytes)?;
    if !value.is_object() || value["version"].as_u64() != Some(1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "incompatible frame",
        ));
    }
    Ok(value)
}

/// Write one bounded versioned frame.
///
/// # Errors
/// Reports serialization, size, or stream failures before acknowledgment.
pub fn write(writer: &mut impl Write, mut value: Value) -> io::Result<()> {
    if !value.is_object() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame must be an object",
        ));
    }
    value["version"] = 1.into();
    let bytes = serde_json::to_vec(&value)?;
    let size = u32::try_from(bytes.len())
        .ok()
        .filter(|_| bytes.len() <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "oversized frame"))?;
    writer.write_all(&size.to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn partial_and_incompatible_frames() {
        for bytes in [vec![0], vec![0, 0, 0, 3, b'{'], vec![255; 4]] {
            assert!(read(&mut bytes.as_slice()).is_err());
        }
        for value in [
            json!({"version": true}),
            json!({"version": 1.0}),
            json!({"version": 2}),
        ] {
            assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    proptest! {
        #[test]
        fn frames_round_trip(input in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let value = json!({"input": input});
            let mut bytes = Vec::new();
            write(&mut bytes, value.clone()).unwrap();
            let actual = read(&mut bytes.as_slice()).unwrap().unwrap();
            prop_assert_eq!(&actual["input"], &value["input"]);
        }
        #[test]
        fn arbitrary_payload_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let _ = decode(&bytes);
            let _ = read(&mut bytes.as_slice());
        }
    }
}
