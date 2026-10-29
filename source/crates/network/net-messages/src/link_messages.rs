use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use campfire_net_bytes::Writer;
use campfire_net_core::NetId;
use campfire_net_transport::Transport;

use crate::message_registry::MessageRegistry;
use crate::message_type_id::MessageTypeId;
use crate::remote_entity_map::RemoteEntityMap;

/// A link's message senders and receivers by type, and the link's [`RemoteEntityMap`]: its own
/// pairs, such as the two link entities from `SenderHello`, which replicon never sees. Only a
/// client's side maps through it, after replicon's
/// [`ServerEntityMap`](bevy_replicon::shared::server_entity_map::ServerEntityMap)
/// (11-net-stack/api.md, Only a client maps the entities a message names).
#[derive(Debug, Component, Default, Reflect)]
#[require(Transport)]
pub struct LinkMessages {
    /// The net ids of the message types whose sender components are on this entity.
    #[reflect(ignore)]
    pub(crate) senders: Vec<NetId>,
    /// The net ids of the message types whose receiver components are on this entity.
    #[reflect(ignore)]
    pub(crate) receivers: Vec<NetId>,
    /// The writer this link's messages and events are serialized with, so its chunks are kept
    /// once for each link.
    #[reflect(ignore)]
    pub(crate) writer: Writer,
    /// The link's own entity pairs. See the [struct-level docs](LinkMessages).
    pub entity_mapper: RemoteEntityMap,
}

/// Which of a [`LinkMessages`]'s lists a sender or receiver component goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointList {
    Send,
    Receive,
}

/// A component through which a link sends or receives messages of one type, which the link's
/// [`LinkMessages`] lists while the component is there.
pub(crate) trait ListedEndpoint {
    const LIST: EndpointList;

    fn message_type_id() -> MessageTypeId;
}

impl LinkMessages {
    /// Lists the endpoint `T` as it is added: `T` requires [`LinkMessages`], so the lists are
    /// there, and its message type is registered before.
    pub(crate) fn list<T: ListedEndpoint>(mut world: DeferredWorld<'_>, context: HookContext) {
        let entry = Self::net_id(&world, T::message_type_id());
        let mut link_messages = world
            .get_mut::<Self>(context.entity)
            .expect("a message endpoint requires its LinkMessages");
        let list = link_messages.list_mut(T::LIST);
        if !list.contains(&entry) {
            list.push(entry);
        }
    }

    /// Takes the endpoint `T` off the lists as it is removed, so they never name a removed
    /// component.
    pub(crate) fn unlist<T: ListedEndpoint>(mut world: DeferredWorld<'_>, context: HookContext) {
        let entry = Self::net_id(&world, T::message_type_id());
        let Some(mut link_messages) = world.get_mut::<Self>(context.entity) else {
            return;
        };
        link_messages
            .list_mut(T::LIST)
            .retain(|known| *known != entry);
    }

    fn net_id(world: &DeferredWorld<'_>, type_id: MessageTypeId) -> NetId {
        world
            .get_resource::<MessageRegistry>()
            .and_then(|registry| registry.net_id(type_id))
            .expect("a message is registered before its sender or receiver is added")
    }

    const fn list_mut(&mut self, endpoint: EndpointList) -> &mut Vec<NetId> {
        match endpoint {
            EndpointList::Send => &mut self.senders,
            EndpointList::Receive => &mut self.receivers,
        }
    }
}
