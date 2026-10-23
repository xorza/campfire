use super::*;

use crate::writer::Writer;

#[test]
fn a_byte_string_reads_back_as_it_was_written() {
    let a: Bytes = vec![7; 100].into();
    let mut writer = Writer::with_capacity(5);
    a.to_bytes(&mut writer);

    let mut reader = Reader::from(writer.into_bytes());
    let read = Bytes::from_bytes(&mut reader).unwrap();
    assert_eq!(a, read);
}

#[test]
fn a_truncated_value_fails_to_read() {
    let mut writer = Writer::default();
    writer.write_varint(10);
    writer.write_bytes(&[7]);

    let mut reader = Reader::from(writer.into_bytes());
    assert!(Bytes::from_bytes(&mut reader).is_err());
}

/// A peer's count beyond the bytes left is refused before any allocation: a varint of
/// 2^62 - 1 followed by one byte would otherwise reserve exabytes.
#[test]
fn counts_beyond_the_remaining_bytes_are_refused() {
    let mut writer = Writer::default();
    writer.write_varint(VarInt::MAX.get());
    writer.write_bytes(&[7]);
    let bytes = writer.into_bytes();
    assert!(matches!(
        Bytes::from_bytes(&mut Reader::from(bytes)),
        Err(SerializationError::Truncated)
    ));
}
