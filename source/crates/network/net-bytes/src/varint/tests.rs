use crate::reader::ReadVarInt;
use crate::writer::WriteInteger;
use std::io;

#[test]
fn a_one_byte_varint_round_trips() {
    // TEST WITH 1
    let mut writer = vec![];

    let val = 1;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 1);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);

    // TEST WITH 63
    let mut writer = vec![];

    let val = 63;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 1);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);
}

#[test]
fn a_two_byte_varint_round_trips() {
    let mut writer = vec![];

    let val = 64;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 2);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);

    let mut writer = vec![];

    let val = 16383;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 2);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);
}

#[test]
fn a_four_byte_varint_round_trips() {
    let mut writer = vec![];

    let val = 16384;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 4);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);

    let mut writer = vec![];

    let val = 1_073_741_823;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 4);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);
}

#[test]
fn an_eight_byte_varint_round_trips() {
    let mut writer = vec![];

    let val = 1_073_741_828;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 8);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);

    let mut writer = vec![];

    let val = 4_611_686_018_427_387_903;
    writer.write_varint(val).unwrap();
    assert_eq!(writer.len(), 8);

    let mut reader = io::Cursor::new(writer);
    let read_val = reader.read_varint().unwrap();
    assert_eq!(val, read_val);
}

/// The sample encodings of RFC 9000, appendix A.1, whose variable-length integer this is. The
/// two-byte `0x4025` is a non-minimal 37: it reads back, though the writer never produces it.
#[test]
fn encodings_match_rfc_9000() {
    let samples: [(&[u8], u64); 4] = [
        (&[0x25], 37),
        (&[0x7b, 0xbd], 15_293),
        (&[0x9d, 0x7f, 0x3e, 0x7d], 494_878_333),
        (
            &[0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c],
            151_288_809_941_952_652,
        ),
    ];
    for (encoded, value) in samples {
        let mut writer = vec![];
        writer.write_varint(value).unwrap();
        assert_eq!(writer, encoded, "{value}");
        assert_eq!(io::Cursor::new(encoded).read_varint().unwrap(), value);
    }
    assert_eq!(io::Cursor::new([0x40, 0x25]).read_varint().unwrap(), 37);
}
