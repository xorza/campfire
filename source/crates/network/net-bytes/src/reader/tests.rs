use super::*;

use crate::writer::WriteInteger;

#[test]
fn an_integer_reads_back_as_it_was_written() {
    let mut writer = vec![];
    writer.write_u8(1);
    writer.write_u16(2);
    writer.write_u32(3);
    writer.write_u64(4);

    let mut reader = Reader::from(writer);

    assert_eq!(reader.read_u8().unwrap(), 1);
    assert_eq!(reader.read_u16().unwrap(), 2);
    assert_eq!(reader.read_u32().unwrap(), 3);
    assert_eq!(reader.read_u64().unwrap(), 4);
    assert_eq!(reader.remaining(), 0);
    assert!(reader.read_u8().is_err());
}

/// A split takes the next bytes and leaves the rest, and a split past the end is refused and
/// takes nothing.
#[test]
fn a_split_takes_the_next_bytes() {
    let mut reader = Reader::from(vec![1, 2, 3, 4, 5]);
    assert_eq!(reader.split_len(2).unwrap(), [1, 2].as_slice());
    assert!(reader.split_len(4).is_err());
    assert_eq!(reader.remaining(), 3);
    assert_eq!(reader.read_u8(), Ok(3));
    assert_eq!(reader.split_len(2).unwrap(), [4, 5].as_slice());
    assert!(!reader.has_remaining());
}

/// Three bytes follow the count: a count of 3 is the largest the buffer can hold.
#[test]
fn read_count_allows_up_to_the_remaining_bytes() {
    for (count, fits) in [(0, true), (3, true), (4, false)] {
        let mut writer = vec![];
        writer.write_varint(count);
        writer.extend_from_slice(&[1, 2, 3]);
        let mut reader = Reader::from(writer);
        let read = reader.read_count();
        if fits {
            assert_eq!(read.unwrap(), usize::try_from(count).unwrap());
            assert_eq!(reader.remaining(), 3);
        } else {
            assert!(read.is_err());
        }
    }
}

#[test]
fn a_varint_reads_back_without_seek() {
    let values = [
        0,
        63,
        64,
        16_383,
        16_384,
        1_073_741_823,
        1_073_741_824,
        4_611_686_018_427_387_903,
    ];
    let mut writer = vec![];
    for value in values {
        writer.write_varint(value);
    }

    let mut reader = Reader::from(writer);
    for value in values {
        assert_eq!(reader.read_varint().unwrap(), value);
    }
}
