use bevy_ecs::prelude::*;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_core::PositiveTickDelta;
use campfire_net_messages::entity_bytes;

/// Event emitted when the receiver learns the sender's replication interval.
///
/// This is sent as part of the initial handshake so the receiver can align
/// its interpolation and timeline logic with the sender's send rate.
#[derive(Event, Debug)]
pub struct SenderHello {
    /// How often the sender emits replication updates.
    pub send_interval: PositiveTickDelta,
    /// The link entity that sent this hello.
    pub link: Entity,
}

impl ToBytes for SenderHello {
    fn bytes_len(&self) -> usize {
        self.send_interval.bytes_len() + entity_bytes::len(self.link)
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        self.send_interval.to_bytes(buffer);
        entity_bytes::write(self.link, buffer);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let send_interval = PositiveTickDelta::from_bytes(buffer)?;
        let link = entity_bytes::read(buffer)?;
        Ok(Self {
            send_interval,
            link,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::internals::replication_stack::ReplicationStack;
    use bevy_ecs::world::EntityRef;
    use campfire_net_messages::MessageManager;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;

    /// Each end's hello maps the peer's link entity to its own, in the messages layer's map:
    /// replicon's map holds only replicated entities.
    #[test]
    fn hello_maps_the_link_entities_both_ways() {
        let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());
        // The hello goes on connection and arrives in the next frames.
        stepper.frame_step(2);
        let client = stepper.client_entity(0);
        let client_of = stepper.client_of_entity(0);
        let local = |entity: EntityRef<'_>, remote| {
            entity
                .get::<MessageManager>()
                .unwrap()
                .entity_mapper
                .get_local(remote)
        };
        assert_eq!(local(stepper.client_of(0), client), Some(client_of));
        assert_eq!(local(stepper.client(0), client_of), Some(client));
    }
}
