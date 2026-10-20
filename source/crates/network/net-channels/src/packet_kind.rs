use campfire_net_payload::PayloadKind;

use crate::packet_error::PacketError;

/// What a packet's payload holds, which the packet session carries in its 2 bits of payload kind
/// (11-net-stack/packet-session.md, The packet session carries 2 bits of its payload's kind, which
/// it does not read).
///
/// Each payload starts with the tick, then:
/// - `Data`: batches of a channel ID, a message count and that many messages, to the end. Channel
///   ID zero is a valid channel and is not an end marker.
/// - `DataFragment`: a channel ID and one fragment, then batches as `Data` has.
/// - `DataCompressed`: a `Data` payload whose bytes after the tick are compressed. A fragmented
///   message is compressed whole before it is split, so fragment packets have no compressed form.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum PacketKind {
    Data,
    DataFragment,
    DataCompressed,
}

impl From<PacketKind> for PayloadKind {
    fn from(kind: PacketKind) -> Self {
        Self::new(match kind {
            PacketKind::Data => 0,
            PacketKind::DataFragment => 1,
            PacketKind::DataCompressed => 2,
        })
    }
}

impl TryFrom<PayloadKind> for PacketKind {
    type Error = PacketError;

    fn try_from(kind: PayloadKind) -> Result<Self, Self::Error> {
        match kind.bits() {
            0 => Ok(Self::Data),
            1 => Ok(Self::DataFragment),
            2 => Ok(Self::DataCompressed),
            bits => Err(PacketError::UnknownPacketKind(bits)),
        }
    }
}

impl PacketKind {
    pub(crate) const fn is_compressed(self) -> bool {
        matches!(self, Self::DataCompressed)
    }

    pub(crate) const fn uncompressed_variant(self) -> Self {
        match self {
            Self::Data | Self::DataCompressed => Self::Data,
            Self::DataFragment => Self::DataFragment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each kind takes its own of the 2 bits and reads back from them; the fourth value is no
    /// kind.
    #[test]
    fn each_kind_reads_back_from_its_bits() {
        for (kind, bits) in [
            (PacketKind::Data, 0),
            (PacketKind::DataFragment, 1),
            (PacketKind::DataCompressed, 2),
        ] {
            let payload_kind = PayloadKind::from(kind);
            assert_eq!(payload_kind.bits(), bits);
            assert_eq!(PacketKind::try_from(payload_kind).unwrap(), kind);
        }
        assert!(matches!(
            PacketKind::try_from(PayloadKind::new(3)),
            Err(PacketError::UnknownPacketKind(3))
        ));
    }

    #[test]
    fn a_compressed_kind_reads_as_its_uncompressed_one() {
        assert_eq!(
            PacketKind::DataCompressed.uncompressed_variant(),
            PacketKind::Data
        );
        assert_eq!(
            PacketKind::DataFragment.uncompressed_variant(),
            PacketKind::DataFragment
        );
    }
}
