//! A link's channels on its entity.

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Insert};
use bevy_ecs::observer::On;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Commands, Query, Res};
use bevy_ecs::world::DeferredWorld;
use campfire_net_channels::{Channels, SendGate};
use campfire_net_link::{Link, Linked};

use crate::protocol_channel::ProtocolChannel;
use crate::protocol_check::ProtocolCheck;
use crate::protocol_hashes::ProtocolHashes;

/// The [`Channels`] of a link, on its entity: the largest payload its packet session hands over
/// sizes their fragments, when the transport comes and at each link (11-net-stack/layering.md, The
/// channels are a problem crate, and `Transport` the component that wraps them).
#[derive(Component, Debug, Default, Deref, DerefMut)]
#[component(on_add = Self::on_add)]
#[require(Link)]
pub struct Transport(pub Channels);

impl From<Channels> for Transport {
    fn from(channels: Channels) -> Self {
        Self(channels)
    }
}

impl Transport {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        let Some(max_payload) = world
            .get::<Link>(context.entity)
            .map(|link| link.packet_session.max_payload())
            .filter(|max_payload| *max_payload > 0)
        else {
            return;
        };
        let Some(mut transport) = world.get_mut::<Self>(context.entity) else {
            return;
        };
        transport.size_fragments(max_payload);
    }

    /// A new packet session: the channels start their tick again, its largest payload sizes
    /// their fragments, and the protocol check starts.
    pub(crate) fn on_linked(
        trigger: On<'_, '_, Insert<Linked>>,
        mut query: Query<'_, '_, (&Link, &mut Self, Has<ProtocolCheck>)>,
        hashes: Res<'_, ProtocolHashes>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok((link, mut transport, checking)) = query.get_mut(trigger.entity) else {
            return;
        };
        transport.link(link.packet_session.max_payload());
        if !checking {
            transport.start_check(trigger.entity, &hashes, &mut commands);
        }
    }

    /// Starts the protocol check of the link `entity`, when its transport opened the check's
    /// channel: sends this end's hashes, and holds every other channel until the peer's arrive
    /// and match (11-net-stack/transport.md, The protocol check goes first on a reserved channel).
    /// Its caller starts it once for each packet session, on a link with no `ProtocolCheck`.
    pub(crate) fn start_check(
        &mut self,
        entity: Entity,
        hashes: &ProtocolHashes,
        commands: &mut Commands<'_, '_>,
    ) {
        if !self.sends_on(ProtocolChannel::ID) {
            return;
        }
        self.set_send_gate(SendGate::Only(ProtocolChannel::ID));
        let priority = ProtocolChannel::setup().priority;
        self.send(ProtocolChannel::ID, hashes.bytes(), priority);
        commands.entity(entity).insert(ProtocolCheck::Waiting);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_net_channels::{Channel, ChannelTypeId};
    use campfire_net_core::NetId;

    use crate::channel_registry::ChannelRegistry;
    use crate::transport::Transport;

    impl Transport {
        /// Adds a sending channel of `C` with its registered setup and wire ID.
        pub fn add_channel_send_from_registry<C: Channel>(&mut self, registry: &ChannelRegistry) {
            let channel_id = Self::registered::<C>(registry);
            self.add_channel_send::<C>(*registry.setup(channel_id).unwrap(), channel_id);
        }

        /// Adds a receiving channel of `C` with its registered setup and wire ID.
        pub fn add_channel_receive_from_registry<C: Channel>(
            &mut self,
            registry: &ChannelRegistry,
        ) {
            let channel_id = Self::registered::<C>(registry);
            self.add_channel_receive::<C>(*registry.setup(channel_id).unwrap(), channel_id);
        }

        fn registered<C: Channel>(registry: &ChannelRegistry) -> NetId {
            registry
                .net_id(ChannelTypeId::of::<C>())
                .unwrap_or_else(|| {
                    panic!("channel {} is not registered", core::any::type_name::<C>())
                })
        }
    }
}

#[cfg(test)]
mod tests;
