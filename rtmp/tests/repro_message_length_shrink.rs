extern crate rml_rtmp;

use rml_rtmp::chunk_io::{ChunkDeserializationError, ChunkDeserializer};

// csid 3, type-0 header: message_length = 200 (> default max chunk size 128), first 128 bytes of payload;
// then a type-1 header on the same csid that changes message_length to 10 while 128 bytes are buffered.
fn crafted() -> Vec<u8> {
    let mut b = vec![0x03, 0, 0, 0, 0x00, 0x00, 0xC8, 0x09, 1, 0, 0, 0];
    b.extend(std::iter::repeat(0xAA).take(128));
    b.extend(vec![0x43, 0, 0, 0, 0x00, 0x00, 0x0A, 0x09]);
    b.extend(std::iter::repeat(0xBB).take(10));
    b
}

#[test]
fn repro_message_length_shrink_mid_message() {
    let mut d = ChunkDeserializer::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.get_next_message(&crafted())));
    match r {
        Ok(Err(ChunkDeserializationError::MessageLengthBelowReceivedPayload { .. })) => {}
        Ok(other) => panic!("unexpected result: {:?}", other.map(|m| m.map(|m| m.data.len()))),
        Err(_) => panic!("deserializer panicked on peer-controlled input"),
    }
}

// type-1 chunk with timestamp delta field 0xFFFFFF and an extended timestamp below 0xFFFFFF
#[test]
fn repro_extended_timestamp_below_field_max() {
    let mut b = vec![0x03, 0, 0, 0, 0x00, 0x00, 0x01, 0x09, 1, 0, 0, 0, 0x00];
    b.extend(vec![0x43, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x01, 0x09, 0x00, 0x00, 0x00, 0x05, 0x00]);
    let mut d = ChunkDeserializer::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let first = d.get_next_message(&b).unwrap().unwrap();
        let second = d.get_next_message(&[]).unwrap().unwrap();
        (first.timestamp.value, second.timestamp.value)
    }));
    assert_eq!(r.ok(), Some((0, 5)), "extended timestamp delta must be applied mod 2^32");
}
