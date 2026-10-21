use bevy_ecs::prelude::*;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_core::PositiveTickSpan;
use campfire_net_messages::WireEntity;

/// Event emitted when the receiver learns the sender's replication interval.
///
/// This is sent as part of the initial handshake so the receiver can align
/// its interpolation and timeline logic with the sender's send rate.
#[derive(Event, Debug)]
pub struct SenderHello {
    /// How often the sender emits replication updates.
    pub send_interval: PositiveTickSpan,
    /// The link entity that sent this hello.
    pub link: Entity,
}

impl ToBytes for SenderHello {
    fn bytes_len(&self) -> usize {
        self.send_interval.bytes_len() + WireEntity(self.link).bytes_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        self.send_interval.to_bytes(buffer);
        WireEntity(self.link).to_bytes(buffer);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let send_interval = PositiveTickSpan::from_bytes(buffer)?;
        let WireEntity(link) = WireEntity::from_bytes(buffer)?;
        Ok(Self {
            send_interval,
            link,
        })
    }
}

#[cfg(test)]
mod tests;
