use super::*;

use crate::fragment_body::FragmentBody;

use crate::fragment_count::FragmentCount;
use crate::fragment_index::FragmentIndex;
use crate::single_body::SingleBody;
use bytes::Bytes;

#[test]
fn a_message_id_wraps_at_u16_max() {
    let last = MessageId(u16::MAX);
    let first = MessageId(0);
    let mut next = last;

    next += 1;

    assert_eq!(next, first);
    assert_eq!(last + 1_i16, first);
    assert_eq!(first - 1, last);
    assert_eq!(first - last, 1);
    assert_eq!(MessageId(1) - first, 1);
    // Half the space less one, 32,767, is still a forward distance; an id takes two bytes.
    assert_eq!(MessageId(i16::MAX.cast_unsigned()) - first, i16::MAX);
    let mut writer = campfire_net_bytes::Writer::default();
    MessageId(513).to_bytes(&mut writer);
    assert_eq!(MessageId(513).bytes_len(), 2);
    let mut reader = Reader::from(writer.take_written());
    assert_eq!(MessageId::from_bytes(&mut reader).unwrap(), MessageId(513));
}

/// A message with no id takes its length and bytes: 1 + 10; one with an id takes 2 more, with no
/// tag before them. Each reads back with its channel's rule.
#[test]
fn a_single_body_reads_back_in_its_bytes_len() {
    for (id, len) in [(None, 11), (Some(MessageId(1)), 13)] {
        let data = SingleBody::new(id, vec![7u8; 10].into());
        let mut writer = vec![];
        data.write(&mut writer);
        assert_eq!([writer.len(), data.bytes_len()], [len, len]);
        let mut reader = writer.into();
        let decoded = SingleBody::read(&mut reader, id.is_some()).unwrap();
        assert_eq!(decoded, data);
    }
}

/// A fragment body is its id's 2 bytes, a byte each for its index, its count and its length,
/// and its 10 bytes, 15 in all, the first fragment's as any other's.
#[test]
fn a_fragment_body_reads_back_in_its_bytes_len() {
    for index in [0, 2] {
        let data = FragmentBody {
            message_id: MessageId(0),
            fragment_index: FragmentIndex(index),
            fragments: FragmentCount(3),
            bytes: Bytes::from(vec![0; 10]),
        };
        let mut writer = vec![];
        data.to_bytes(&mut writer);
        assert_eq!([writer.len(), data.bytes_len()], [15, 15]);
        let decoded = FragmentBody::from_bytes(&mut writer.into()).unwrap();
        assert_eq!(decoded, data);
    }
}
