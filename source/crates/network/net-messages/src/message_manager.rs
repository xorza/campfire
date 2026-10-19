use crate::endpoint_side::EndpointSide;
use crate::message_kind::MessageKind;
use crate::message_registry::MessageRegistry;
use crate::remote_entity_map::RemoteEntityMap;
use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use campfire_net_bytes::Writer;
use campfire_net_core::NetId;
use campfire_net_transport::Transport;

/// Manages sending and receiving messages for an entity.
///
/// This component is added to entities that need to send or receive messages.
/// It keeps track of the `MessageSender<M>` and `MessageReceiver<M>` components
/// attached to the entity, allowing the messaging system to interact with them.
/// It also holds the link's [`RemoteEntityMap`]: its own pairs, such as the two link entities
/// from `SenderHello`, which replicon never sees. Only a client's side maps through it, after
/// replicon's [`ServerEntityMap`](bevy_replicon::shared::server_entity_map::ServerEntityMap).
#[derive(Debug, Component, Default, Reflect)]
#[require(Transport)]
pub struct MessageManager {
    /// The `MessageSender<M>` and
    /// `EventSender<M>` components on this entity.
    #[reflect(ignore)]
    pub(crate) senders: Vec<LinkEndpoint>,
    /// The typed receiver components on this entity, a host client's own side's included.
    #[reflect(ignore)]
    pub(crate) receivers: Vec<LinkEndpoint>,
    /// The writer this link's messages and events are serialized with, so its chunks are kept
    /// once for each link.
    #[reflect(ignore)]
    pub(crate) writer: Writer,
    /// The link's own entity pairs. See the [struct-level docs](MessageManager).
    pub entity_mapper: RemoteEntityMap,
}

/// One sender or receiver component of a link: the message type it serves, by net id, and the
/// side of the entity it serves, which together name its component in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinkEndpoint {
    pub(crate) net_id: NetId,
    pub(crate) side: EndpointSide,
}

/// Which of a [`MessageManager`]'s lists a sender or receiver component goes in, and the side
/// of the entity it serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endpoint {
    Send(EndpointSide),
    Receive(EndpointSide),
}

impl MessageManager {
    /// The `on_add` hook of an `endpoint` component of `kind`: the component requires its
    /// manager, so the manager is there. A type is registered before its sender or receiver is
    /// added.
    pub(crate) fn on_endpoint_add(
        mut world: DeferredWorld<'_>,
        context: HookContext,
        endpoint: Endpoint,
        kind: MessageKind,
    ) {
        let entry = Self::entry(&world, endpoint, kind);
        let mut manager = world
            .get_mut::<Self>(context.entity)
            .expect("a message endpoint requires its manager");
        let list = manager.list_mut(endpoint);
        if !list.contains(&entry) {
            list.push(entry);
        }
    }

    /// The `on_remove` hook of an `endpoint` component of `kind`, so the lists never name a
    /// removed component.
    pub(crate) fn on_endpoint_remove(
        mut world: DeferredWorld<'_>,
        context: HookContext,
        endpoint: Endpoint,
        kind: MessageKind,
    ) {
        let entry = Self::entry(&world, endpoint, kind);
        let Some(mut manager) = world.get_mut::<Self>(context.entity) else {
            return;
        };
        manager.list_mut(endpoint).retain(|known| *known != entry);
    }

    fn entry(world: &DeferredWorld<'_>, endpoint: Endpoint, kind: MessageKind) -> LinkEndpoint {
        let net_id = world
            .get_resource::<MessageRegistry>()
            .and_then(|registry| registry.net_id(kind))
            .expect("a message is registered before its sender or receiver is added");
        let (Endpoint::Send(side) | Endpoint::Receive(side)) = endpoint;
        LinkEndpoint { net_id, side }
    }

    const fn list_mut(&mut self, endpoint: Endpoint) -> &mut Vec<LinkEndpoint> {
        match endpoint {
            Endpoint::Send(_) => &mut self.senders,
            Endpoint::Receive(_) => &mut self.receivers,
        }
    }
}
