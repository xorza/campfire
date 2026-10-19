use crate::fragment_data::FragmentData;
use crate::single_data::SingleData;
use crate::tick_decoder::TickDecoder;
use bevy_platform::collections::HashMap;
use bytes::Bytes;
use campfire_net_core::NetId;

use super::*;

use crate::packet_error::PacketError;

use campfire_net_bytes::ReadInteger;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;

impl Packet {
    /// The messages of an uncompressed packet by channel, each of a channel whose messages
    /// carry an id when `has_id` says so.
    pub(crate) fn parse_packet_payload(
        self,
        has_id: impl Fn(NetId) -> bool,
    ) -> Result<HashMap<NetId, Vec<Bytes>>, PacketError> {
        let mut payload = self.payload.freeze();
        let _ = payload.split_to(OutgoingPayload::HEADROOM);
        let mut cursor = payload.into();
        let mut res: HashMap<NetId, Vec<Bytes>> = HashMap::default();
        TickDecoder::default().read(&mut cursor)?;

        if self.kind == PacketKind::DataFragment {
            // read the fragment data
            let channel_id = NetId::from_bytes(&mut cursor)?;
            let fragment_data = FragmentData::from_bytes(&mut cursor)?;
            res.entry(channel_id).or_default().push(fragment_data.bytes);
        }
        while cursor.has_remaining() {
            let channel_id = NetId::from_bytes(&mut cursor)?;
            let num_messages = cursor.read_u8().map_err(SerializationError::from)?;
            for _ in 0..num_messages {
                let single_data = SingleData::read(&mut cursor, has_id(channel_id))?;
                res.entry(channel_id).or_default().push(single_data.bytes);
            }
        }
        Ok(res)
    }
}

#[test]
fn fragment_size_uses_its_own_encoded_length_and_is_maximal() {
    assert_eq!(
        fragment_size_for_max_payload(MIN_FRAGMENT_PAYLOAD - 1),
        None
    );
    assert_eq!(fragment_size_for_max_payload(MIN_FRAGMENT_PAYLOAD), Some(1));

    let fragment_size = fragment_size_for_max_payload(DEFAULT_MAX_PAYLOAD).unwrap();
    let packet_size = FIXED_FRAGMENT_PACKET_BYTES
        + VarInt::new(fragment_size as u64).encoded_len()
        + fragment_size;
    assert!(packet_size <= DEFAULT_MAX_PAYLOAD);

    let next_fragment_size = fragment_size + 1;
    let next_packet_size = FIXED_FRAGMENT_PACKET_BYTES
        + VarInt::new(next_fragment_size as u64).encoded_len()
        + next_fragment_size;
    assert!(next_packet_size > DEFAULT_MAX_PAYLOAD);
}
