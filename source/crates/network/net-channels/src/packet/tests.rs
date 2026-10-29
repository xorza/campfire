use bevy_platform::collections::HashMap;
use bytes::Bytes;
use campfire_net_bytes::ToBytes;
use campfire_net_core::NetId;

use super::*;
use crate::error::ReceiveError;
use crate::fragment_body::FragmentBody;
use crate::single_body::SingleBody;
use crate::tick_decoder::TickDecoder;

impl Packet {
    /// The messages of a packet by channel, each of a channel whose messages
    /// carry an id when `has_id` says so.
    pub(crate) fn parse_packet_payload(
        self,
        has_id: impl Fn(NetId) -> bool,
    ) -> Result<HashMap<NetId, Vec<Bytes>>, ReceiveError> {
        let mut payload = self.payload.freeze();
        let _ = payload.split_to(OutgoingPayload::HEADROOM);
        let mut cursor = payload.into();
        let mut res: HashMap<NetId, Vec<Bytes>> = HashMap::default();
        TickDecoder::default().read(&mut cursor)?;

        if self.kind == PacketKind::DataFragment {
            // read the fragment data
            let channel_id = NetId::from_bytes(&mut cursor)?;
            let fragment_body = FragmentBody::from_bytes(&mut cursor)?;
            res.entry(channel_id).or_default().push(fragment_body.bytes);
        }
        while cursor.has_remaining() {
            let channel_id = NetId::from_bytes(&mut cursor)?;
            let messages = cursor.read_u8()?;
            for _ in 0..messages {
                let single_body = SingleBody::read(&mut cursor, has_id(channel_id))?;
                res.entry(channel_id).or_default().push(single_body.bytes);
            }
        }
        Ok(res)
    }
}

#[test]
fn the_fragment_len_counts_its_own_encoding_and_is_the_largest() {
    assert_eq!(fragment_len_for_max_payload(MIN_FRAGMENT_PAYLOAD - 1), None);
    assert_eq!(fragment_len_for_max_payload(MIN_FRAGMENT_PAYLOAD), Some(1));

    let fragment_len = fragment_len_for_max_payload(DEFAULT_MAX_PAYLOAD).unwrap();
    let packet_len =
        FIXED_FRAGMENT_PACKET_BYTES + VarInt::new(fragment_len as u64).encoded_len() + fragment_len;
    assert!(packet_len <= DEFAULT_MAX_PAYLOAD);

    let next_fragment_len = fragment_len + 1;
    let next_packet_len = FIXED_FRAGMENT_PACKET_BYTES
        + VarInt::new(next_fragment_len as u64).encoded_len()
        + next_fragment_len;
    assert!(next_packet_len > DEFAULT_MAX_PAYLOAD);
}
