//! A link's channels on its entity.

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::{HookContext, Insert};
use bevy_ecs::observer::On;
use bevy_ecs::system::Query;
use bevy_ecs::world::DeferredWorld;
use campfire_net_channels::{Channel, ChannelKind, Channels, RefusedMessage};
use campfire_net_core::NetId;
use campfire_net_link::{Link, Linked};
use tracing::{error, trace, warn};

use crate::channel_registry::ChannelRegistry;

/// The [`Channels`] of a link, on its entity: the largest payload its session hands over sizes
/// their fragments, when the transport comes and at each link.
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
            .map(|link| link.session.max_payload())
            .filter(|max_payload| *max_payload > 0)
        else {
            return;
        };
        let Some(mut transport) = world.get_mut::<Self>(context.entity) else {
            return;
        };
        if let Err(error) = transport.size_fragments(max_payload) {
            error!(?error, "invalid link payload for transport");
        }
    }

    /// A new session: the channels start their tick again, and its largest payload sizes their
    /// fragments.
    pub(crate) fn on_linked(
        trigger: On<'_, '_, Insert<Linked>>,
        mut query: Query<'_, '_, (&Link, &mut Self)>,
    ) {
        let Ok((link, mut transport)) = query.get_mut(trigger.entity) else {
            return;
        };
        match transport.link(link.session.max_payload()) {
            Ok(refused) => {
                for RefusedMessage { kind, size, max } in refused {
                    warn!(
                        ?kind,
                        size, max, "dropping a staged message above the new link's message bound"
                    );
                }
            }
            Err(error) => error!(?error, "invalid link payload for transport"),
        }
    }

    /// Adds a sending channel of `C` with its registered config and wire ID.
    pub fn add_channel_send_from_registry<C: Channel>(&mut self, registry: &ChannelRegistry) {
        trace!(
            "Adding sender from registry for channel {}",
            core::any::type_name::<C>()
        );
        let channel_id = Self::registered::<C>(registry);
        self.add_channel_send::<C>(*registry.config(channel_id).unwrap(), channel_id);
    }

    /// Adds a receiving channel of `C` with its registered config and wire ID.
    pub fn add_channel_receive_from_registry<C: Channel>(&mut self, registry: &ChannelRegistry) {
        let channel_id = Self::registered::<C>(registry);
        self.add_channel_receive::<C>(*registry.config(channel_id).unwrap(), channel_id);
    }

    fn registered<C: Channel>(registry: &ChannelRegistry) -> NetId {
        registry
            .net_id(ChannelKind::of::<C>())
            .unwrap_or_else(|| panic!("channel {} is not registered", core::any::type_name::<C>()))
    }
}

#[cfg(test)]
mod tests;
