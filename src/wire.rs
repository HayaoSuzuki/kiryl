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

    #[rstest::rstest]
    #[case(vec![0])]
    #[case(vec![0, 0, 0, 3, b'{'])]
    #[case(vec![255; 4])]
    fn partial_frames(#[case] bytes: Vec<u8>) {
        assert!(read(&mut bytes.as_slice()).is_err());
    }
    #[rstest::rstest]
    #[case(json!({"version": true}))]
    #[case(json!({"version": 1.0}))]
    #[case(json!({"version": 2}))]
    fn incompatible_frames(#[case] value: serde_json::Value) {
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    struct ReadFailure;
    impl Read for ReadFailure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected read failure"))
        }
    }
    struct FlushFailure;
    impl Write for FlushFailure {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }
    #[test]
    fn initial_read_failure_propagates() {
        assert_eq!(
            read(&mut ReadFailure).unwrap_err().to_string(),
            "injected read failure"
        );
    }
    #[test]
    fn empty_stream_is_clean_eof() {
        assert!(read(&mut [].as_slice()).unwrap().is_none());
    }
    #[test]
    fn zero_length_payload_is_rejected() {
        assert!(read(&mut [0; 4].as_slice()).is_err());
    }
    #[rstest::rstest]
    #[case(MAX_FRAME, true)]
    #[case(MAX_FRAME + 1, false)]
    fn payload_size_boundary(#[case] length: usize, #[case] accepted: bool) {
        let mut bytes = br#"{"version":1}"#.to_vec();
        bytes.resize(length, b' ');
        assert_eq!(decode(&bytes).is_ok(), accepted);
    }
    #[rstest::rstest]
    #[case(json!(null))]
    #[case(json!([1]))]
    #[case(json!("text"))]
    fn writing_nonobject_is_rejected(#[case] value: Value) {
        let mut writer = Vec::new();
        assert_eq!(
            write(&mut writer, value).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(writer, Vec::<u8>::new());
    }
    #[rstest::rstest]
    fn bounded_destination_write_failure_propagates(#[values(0, 4)] capacity: usize) {
        let mut bytes = vec![0; capacity];
        let mut writer = io::Cursor::new(bytes.as_mut_slice());
        assert_eq!(
            write(&mut writer, json!({})).unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
    }
    #[test]
    fn flush_failure_propagates() {
        assert_eq!(
            write(&mut FlushFailure, json!({})).unwrap_err().to_string(),
            "injected flush failure"
        );
    }
    #[test]
    fn writing_oversized_object_fails_before_header() {
        let mut writer = Vec::new();
        assert_eq!(
            write(&mut writer, json!({"data": "x".repeat(MAX_FRAME)}))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(writer, Vec::<u8>::new());
    }
    #[test]
    fn writing_maximum_size_object_round_trips() {
        let overhead = serde_json::to_vec(&json!({"data":"", "version":1}))
            .unwrap()
            .len();
        let value = json!({"data":"x".repeat(MAX_FRAME - overhead), "version":1});
        let mut bytes = Vec::new();
        write(&mut bytes, value.clone()).unwrap();
        assert_eq!(bytes.len(), MAX_FRAME + 4);
        assert_eq!(read(&mut bytes.as_slice()).unwrap(), Some(value));
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
        fn concatenated_frames_preserve_message_boundaries(first in proptest::collection::vec(any::<u8>(), 0..1024), second in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let first_value = json!({"input": first, "version": 1});
            let second_value = json!({"input": second, "version": 1});
            let mut bytes = Vec::new();
            write(&mut bytes, first_value.clone()).unwrap();
            write(&mut bytes, second_value.clone()).unwrap();
            let mut reader = bytes.as_slice();
            prop_assert_eq!(read(&mut reader).unwrap(), Some(first_value));
            prop_assert_eq!(read(&mut reader).unwrap(), Some(second_value));
            prop_assert!(reader.is_empty());
            prop_assert!(read(&mut reader).unwrap().is_none());
        }
        #[test]
        fn arbitrary_payload_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let _ = decode(&bytes);
        }
        #[test]
        fn arbitrary_frame_read_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let _ = read(&mut bytes.as_slice());
        }
    }
}
