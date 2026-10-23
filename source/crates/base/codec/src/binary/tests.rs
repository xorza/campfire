use super::*;
use crate::error::Malformed;

#[test]
fn a_value_decodes_only_from_its_one_encoding() {
    // 300 is the varint 0xAC 0x02: its low seven bits with the high bit set, then 300 >> 7.
    assert_eq!(Binary::encode(&300_u32), [0xAC, 0x02]);
    assert_eq!(Binary::decode::<u32>(&[0xAC, 0x02]), Ok(300));
    // 0 over-long, 0x80 0x00; a byte past the value; bytes that end inside it; a bool of 2.
    assert_eq!(
        Binary::decode::<u32>(&[0x80, 0x00]),
        Err(BinaryError::NotCanonical)
    );
    assert_eq!(
        Binary::decode::<u32>(&[0x01, 0xFF]),
        Err(BinaryError::NotCanonical)
    );
    assert_eq!(Binary::decode::<u32>(&[0xAC]), Err(BinaryError::Truncated));
    assert_eq!(
        Binary::decode::<bool>(&[2]),
        Err(BinaryError::Malformed(Malformed(
            postcard::Error::DeserializeBadBool
        )))
    );
    // A length over-long inside a value: the string "ab" as 0x82 0x00 'a' 'b'.
    assert_eq!(
        Binary::decode::<&str>(&[0x82, 0x00, b'a', b'b']),
        Err(BinaryError::NotCanonical)
    );
    assert_eq!(Binary::decode::<&str>(&[2, b'a', b'b']), Ok("ab"));
}

#[test]
fn a_value_taken_from_the_front_gives_the_rest() {
    let bytes = [2, b'a', b'b', 0xAC, 0x02];
    let first = Binary::take::<&str>(&bytes).unwrap();
    assert_eq!(first.value, "ab");
    assert_eq!(first.rest, [0xAC, 0x02]);
    let second = Binary::take::<u32>(first.rest).unwrap();
    assert_eq!((second.value, second.rest), (300, &[][..]));
    // An over-long value at the front is refused, whatever follows it.
    assert_eq!(
        Binary::take::<u32>(&[0x80, 0x00, 0x05]),
        Err(BinaryError::NotCanonical)
    );
    // The unchecked take reads the same value from it, 0, and makes no such check.
    let unchecked = Binary::take_unchecked::<u32>(&[0x80, 0x00, 0x05]).unwrap();
    assert_eq!((unchecked.value, unchecked.rest), (0, &[0x05][..]));
    assert_eq!(
        Binary::take_unchecked::<u32>(&[0x80]),
        Err(BinaryError::Truncated)
    );
}

#[test]
fn a_tagged_file_reads_back_only_under_its_tag_and_whole() {
    // The tag `t/v1`, then 300, `0xAC 0x02`, written in place of what `out` held.
    let mut out = vec![9; 4];
    Binary::encode_tagged(b"t/v1", &300_u32, &mut out);
    assert_eq!(out, b"t/v1\xAC\x02");
    assert_eq!(Binary::decode_tagged::<u32>(b"t/v1", &out), Ok(300));
    assert_eq!(
        Binary::decode_tagged::<u32>(b"t/v2", &out),
        Err(TaggedError::OtherTag)
    );
    let short = &out[..out.len() - 1];
    assert_eq!(
        Binary::decode_tagged::<u32>(b"t/v1", short),
        Err(TaggedError::Value(BinaryError::Truncated))
    );
    let mut long = out.clone();
    long.push(0);
    assert_eq!(
        Binary::decode_tagged::<u32>(b"t/v1", &long),
        Err(TaggedError::Value(BinaryError::NotCanonical))
    );
}

#[test]
fn an_encode_appends_its_bytes_and_each_value_follows_the_last() {
    // A value longer than a batch of 64: 1 + 100 bytes of the list, 300 in 2, and "end" in
    // 1 + 3, after the 9 `out` held.
    let value = (vec![7_u8; 100], 300_u32, "end");
    let mut appended = vec![9];
    Binary::encode_into(&value, &mut appended);
    assert_eq!(appended.len(), 1 + 1 + 100 + 2 + 1 + 3);
    assert_eq!(appended[..2], [9, 100]);
    // The batched path into a sink writes the same bytes as the append in place.
    let mut batched = vec![9];
    Binary::encode_to(&value, &mut batched);
    assert_eq!(batched, appended);
    assert_eq!(
        Binary::decode::<(Vec<u8>, u32, &str)>(&appended[1..]),
        Ok(value)
    );
    let mut each = Vec::new();
    Binary::encode_each_to([1_u8, 2, 3], &mut each);
    assert_eq!(each, [1, 2, 3]);
}

/// A value with options at each depth: a field, a tuple's inside a `Some`, a vector's
/// element, an enum's variant, and a struct's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Nested {
    first: Option<u8>,
    pair: Option<(Option<u8>, u16)>,
    list: Vec<Option<u32>>,
    kind: Kind,
    none: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    Plain,
    With { rank: Option<u8> },
}

#[test]
fn an_encode_writes_each_some_in_turn_as_none_and_the_rest_as_it_is() {
    let value = Nested {
        first: Some(1),
        pair: Some((Some(2), 3)),
        list: vec![Some(4), None, Some(5)],
        kind: Kind::With { rank: Some(6) },
        none: None,
    };
    // The `Some`s in order: `first`, `pair`, its inner one, the list's two, the rank.
    assert_eq!(Binary::options(&value), 6);
    let dropped = |at| {
        let bytes = Binary::encode_without_option(&value, at)?;
        Some(Binary::decode::<Nested>(&bytes).unwrap())
    };
    let with = |change: fn(&mut Nested)| {
        let mut changed = value.clone();
        change(&mut changed);
        Some(changed)
    };
    assert_eq!(dropped(0), with(|v| v.first = None));
    // The outer `None` takes the inner `Some` with it.
    assert_eq!(dropped(1), with(|v| v.pair = None));
    assert_eq!(dropped(2), with(|v| v.pair = Some((None, 3))));
    assert_eq!(dropped(3), with(|v| v.list[0] = None));
    assert_eq!(dropped(4), with(|v| v.list[2] = None));
    assert_eq!(dropped(5), with(|v| v.kind = Kind::With { rank: None }));
    assert_eq!(dropped(6), None);
    // A value with no `Some` has nothing to write as `None`.
    let plain = (Kind::Plain, None::<u8>, vec![None::<u8>]);
    assert_eq!(Binary::options(&plain), 0);
    assert_eq!(Binary::encode_without_option(&plain, 0), None);
}
