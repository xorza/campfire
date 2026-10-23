use campfire_net_payload::PayloadKind;

use crate::error::ReceiveError;

/// What a packet's payload holds, which the packet session carries in its 2 bits of payload kind
/// (11-net-stack/packet-session.md, The packet session carries 2 bits of its payload's kind, which
/// it does not read).
///
/// Each payload starts with the tick, then:
/// - `Data`: batches of a channel ID, a message count and that many messages, to the end. Channel
///   ID zero is a valid channel and is not an end marker.
/// - `DataFragment`: a channel ID and one fragment, then batches as `Data` has.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub(crate) enum PacketKind {
    Data,
    DataFragment,
}

impl From<PacketKind> for PayloadKind {
    fn from(kind: PacketKind) -> Self {
        Self::new(match kind {
            PacketKind::Data => 0,
            PacketKind::DataFragment => 1,
        })
    }
}

impl TryFrom<PayloadKind> for PacketKind {
    type Error = ReceiveError;

    fn try_from(kind: PayloadKind) -> Result<Self, Self::Error> {
        match kind.bits() {
            0 => Ok(Self::Data),
            1 => Ok(Self::DataFragment),
            bits => Err(ReceiveError::UnknownPacketKind(bits)),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::packet_kind::PacketKind;
    use campfire_net_payload::PayloadKind;

    /// The payload kind of a packet of whole messages.
    pub fn whole_messages() -> PayloadKind {
        PacketKind::Data.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each kind takes its own of the 2 bits and reads back from them; the other two values are
    /// no kind.
    #[test]
    fn each_kind_reads_back_from_its_bits() {
        for (kind, bits) in [(PacketKind::Data, 0), (PacketKind::DataFragment, 1)] {
            let payload_kind = PayloadKind::from(kind);
            assert_eq!(payload_kind.bits(), bits);
            assert_eq!(PacketKind::try_from(payload_kind).unwrap(), kind);
        }
        for bits in [2, 3] {
            assert_eq!(
                PacketKind::try_from(PayloadKind::new(bits)),
                Err(ReceiveError::UnknownPacketKind(bits))
            );
        }
    }
}
