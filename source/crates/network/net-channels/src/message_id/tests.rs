use super::*;

use crate::fragment_compression::FragmentCompression;
use crate::fragment_count::FragmentCount;
use crate::fragment_data::FragmentData;
use crate::fragment_index::FragmentIndex;
use crate::single_data::SingleData;
use bytes::Bytes;

use campfire_net_bytes::ToBytes;

#[test]
fn message_id_arithmetic_wraps_at_u16_max() {
    let last = MessageId(u16::MAX);
    let first = MessageId(0);
    let mut next = last;

    next += 1;

    assert_eq!(next, first);
    assert_eq!(last + 1_i16, first);
    assert_eq!(first - 1, last);
    assert_eq!(first - last, 1);
    assert_eq!(MessageId(1) - first, 1);
}

/// A message with no id takes its length and bytes: 1 + 10; one with an id takes 2 more, with no
/// tag before them. Each reads back with its channel's rule.
#[test]
fn single_data_round_trips_in_its_bytes_len() {
    for (id, len) in [(None, 11), (Some(MessageId(1)), 13)] {
        let data = SingleData::new(id, vec![7u8; 10].into());
        let mut writer = vec![];
        data.write(&mut writer);
        assert_eq!([writer.len(), data.bytes_len()], [len, len]);
        let mut reader = writer.into();
        let decoded = SingleData::read(&mut reader, id.is_some()).unwrap();
        assert_eq!(decoded, data);
    }
}

#[test]
fn fragment_data_round_trips_in_its_bytes_len() {
    let bytes = Bytes::from(vec![0; 10]);
    let data = FragmentData {
        message_id: MessageId(0),
        fragment_index: FragmentIndex(2),
        num_fragments: FragmentCount(3),
        compression: None,
        bytes: bytes.clone(),
    };
    let mut writer = vec![];
    data.to_bytes(&mut writer);

    assert_eq!(writer.len(), data.bytes_len());

    let mut reader = writer.into();
    let decoded = FragmentData::from_bytes(&mut reader).unwrap();
    assert_eq!(decoded, data);

    let initial_data = FragmentData {
        message_id: MessageId(0),
        fragment_index: FragmentIndex(0),
        num_fragments: FragmentCount(3),
        compression: Some(FragmentCompression::Lz4),
        bytes,
    };
    let mut writer = vec![];
    initial_data.to_bytes(&mut writer);

    assert_eq!(writer.len(), initial_data.bytes_len());
    assert_eq!(initial_data.bytes_len(), data.bytes_len() + 1);

    let mut reader = writer.into();
    let decoded = FragmentData::from_bytes(&mut reader).unwrap();
    assert_eq!(decoded, initial_data);
}
