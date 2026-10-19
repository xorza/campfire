use super::*;

use crate::fragment_body::FragmentBody;
use crate::fragment_compression::FragmentCompression;
use crate::fragment_count::FragmentCount;
use crate::fragment_index::FragmentIndex;
use crate::single_body::SingleBody;
use bytes::Bytes;

use campfire_net_bytes::ToBytes;

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

#[test]
fn a_fragment_body_reads_back_in_its_bytes_len() {
    let bytes = Bytes::from(vec![0; 10]);
    let data = FragmentBody {
        message_id: MessageId(0),
        fragment_index: FragmentIndex(2),
        fragments: FragmentCount(3),
        compression: None,
        bytes: bytes.clone(),
    };
    let mut writer = vec![];
    data.to_bytes(&mut writer);

    assert_eq!(writer.len(), data.bytes_len());

    let mut reader = writer.into();
    let decoded = FragmentBody::from_bytes(&mut reader).unwrap();
    assert_eq!(decoded, data);

    let initial_data = FragmentBody {
        message_id: MessageId(0),
        fragment_index: FragmentIndex(0),
        fragments: FragmentCount(3),
        compression: Some(FragmentCompression::Lz4),
        bytes,
    };
    let mut writer = vec![];
    initial_data.to_bytes(&mut writer);

    assert_eq!(writer.len(), initial_data.bytes_len());
    assert_eq!(initial_data.bytes_len(), data.bytes_len() + 1);

    let mut reader = writer.into();
    let decoded = FragmentBody::from_bytes(&mut reader).unwrap();
    assert_eq!(decoded, initial_data);
}
