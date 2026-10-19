use crate::endpoint_side::EndpointSide;
use crate::message::Message;
use crate::message_error::MessageError;
use crate::message_kind::MessageKind;
use crate::message_receive_context::MessageReceiveContext;
use crate::receive_map_view::ReceiveMapView;
use crate::send_map_view::SendMapView;
use crate::send_queue::{DiscardSendFn, SendFn};
use crate::serialize_fns::ContextDeserializeFn;
use crate::serialize_fns::ContextDeserializeFns;
use crate::serialize_fns::ContextSerializeFn;
use crate::serialize_fns::ContextSerializeFns;
use crate::serialize_fns::DeserializeFn;
use crate::serialize_fns::ErasedSerializeFns;
use crate::serialize_fns::SerializeFn;
use bevy_ecs::world::FilteredEntityMut;
use bevy_ecs::{component::ComponentId, entity::MapEntities, error::Result, resource::Resource};
use bevy_reflect::TypePath;
use bevy_utils::prelude::DebugName;
use campfire_common::HashDomain;
use campfire_net_bytes::Reader;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::Writer;
use campfire_net_core::Direction;
use campfire_net_core::Directions;
use campfire_net_core::NetId;
use campfire_net_core::RegistrySeal;
use campfire_net_core::TypeMapper;
use campfire_net_core::{RegistryHash, RegistryHasher};
use std::sync::Arc;
use tracing::trace;

/// The tag of the message registry's hash.
const HASH_DOMAIN: HashDomain<RegistryHash> = HashDomain::new("campfire/message-registry/v1");

/// Delivers one message into its side's receiver among `receivers`, which goes on when absent.
pub(crate) type ReceiveMessageFn = fn(
    receivers: &mut FilteredEntityMut<'_, '_>,
    context: MessageReceiveContext<'_, '_, '_>,
) -> Result<(), MessageError>;

/// Clears the messages of `side`'s receiver among `receivers`.
pub(crate) type ClearMessageFn = fn(receivers: &mut FilteredEntityMut<'_, '_>, side: EndpointSide);

/// Type-erased callback for the serialized-byte receive path of an event.
pub(crate) type ReceiveEventFn =
    fn(context: MessageReceiveContext<'_, '_, '_>) -> Result<(), MessageError>;

#[derive(Debug, Clone)]
pub(crate) struct ReceiveMessageFns {
    /// The `MessageReceiver<M>` component.
    pub(crate) link_component_id: ComponentId,
    /// The `HostMessageReceiver<M>`
    /// component.
    pub(crate) host_component_id: ComponentId,
    pub(crate) receive_message_fn: ReceiveMessageFn,
    pub(crate) message_clear_fn: ClearMessageFn,
}

/// The sender components of a message or event type, one for each side.
#[derive(Debug, Clone, Copy, TypePath)]
pub(crate) struct SendFns {
    /// The `MessageSender<M>` or
    /// `EventSender<M>`.
    pub(crate) link: SenderFns,
    /// The `HostMessageSender<M>` or
    /// `HostEventSender<M>`.
    pub(crate) host: SenderFns,
}

/// One sender component, and the fns that drain it.
#[derive(Debug, Clone, Copy, TypePath)]
pub(crate) struct SenderFns {
    pub(crate) component_id: ComponentId,
    pub(crate) send_fn: SendFn,
    pub(crate) discard_fn: DiscardSendFn,
}

impl SendFns {
    /// The sender of `side`.
    pub(crate) const fn side(&self, side: EndpointSide) -> &SenderFns {
        match side {
            EndpointSide::Link => &self.link,
            EndpointSide::HostClient => &self.host,
        }
    }

    /// Both sender components.
    pub(crate) const fn component_ids(&self) -> [ComponentId; 2] {
        [self.link.component_id, self.host.component_id]
    }
}

#[derive(Debug, Clone, Copy, TypePath)]
pub(crate) struct ReceiveEventFns {
    pub(crate) receive_event_fn: ReceiveEventFn,
}

#[derive(Debug, Clone)]
pub(crate) enum MessageModeFns {
    Message {
        send: SendFns,
        receive: ReceiveMessageFns,
    },
    Event {
        send: SendFns,
        receive: ReceiveEventFns,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct RegisteredMessage {
    pub(crate) mode: MessageModeFns,
    pub(crate) serialize_fns: ErasedSerializeFns,
    /// The directions declared with `add_direction`; empty before any is declared, which
    /// accepts both.
    pub(crate) directions: Directions,
}

impl RegisteredMessage {
    /// Whether the message travels in `direction`: one it declared, or either before it declared
    /// any.
    pub(crate) const fn travels(&self, direction: Direction) -> bool {
        self.directions.is_empty() || self.directions.contains(direction)
    }

    pub(crate) const fn send(&self) -> &SendFns {
        match &self.mode {
            MessageModeFns::Message { send, .. } | MessageModeFns::Event { send, .. } => send,
        }
    }

    /// The receive fns of a message; an event has no receiver, as it is triggered when it
    /// arrives.
    pub(crate) const fn receive_message(&self) -> Option<&ReceiveMessageFns> {
        match &self.mode {
            MessageModeFns::Message { receive, .. } => Some(receive),
            MessageModeFns::Event { .. } => None,
        }
    }

    /// The receiver components of a message, of both sides.
    fn receive_component_ids(&self) -> Option<[ComponentId; 2]> {
        self.receive_message()
            .map(|receive| [receive.link_component_id, receive.host_component_id])
    }
}

/// A [`Resource`] that will keep track of all the [`Message`]s that can be sent over the network.
/// A [`Message`] is any type that is serializable and deserializable.
///
///
/// ### Adding Messages
///
/// You register messages by calling the `add_message` method directly on the App.
///
/// You can provide a [`Direction`](campfire_net_core::Direction), or
/// [`Directions::BOTH`](campfire_net_core::Directions::BOTH), to specify if the message should be
/// sent from the client to the server, from the server to the client, or both. Messages are sent
/// through `MessageSender<M>` and read through `MessageReceiver<M>`. Adding a
/// [`Direction`](campfire_net_core::Direction) installs the sender as a required component on the
/// sending side. The receiving side gets its exact typed receiver lazily when the first payload
/// arrives.
///
///
/// ```rust
/// # use bevy_app::App;
/// # use serde::{Deserialize, Serialize};
/// # use campfire_net_messages::{AppMessageExt, AppEventExt};
/// # use campfire_net_core::Direction;
///
/// #[derive(Serialize, Deserialize)]
/// struct MyMessage;
///
/// fn add_messages(app: &mut App) {
///   app.register_message::<MyMessage>()
///     .add_direction(Direction::ServerToClient);
/// }
/// ```
///
/// ### Customizing Message behaviour
///
/// There are some cases where you might want to define additional behaviour for a message.
/// For example, if the message contains Entities, you need to specify how those en
/// entities will be mapped from the remote world to the local world.
///
/// Provided that your type implements [`MapEntities`], you can extend the protocol to support this
/// behaviour, by calling the `MessageRegistration::add_map_entities` method.
///
/// ```rust
/// # use bevy_app::App;
/// # use serde::{Deserialize, Serialize};
/// # use campfire_net_messages::{AppMessageExt, AppEventExt};
/// # use campfire_net_core::Direction;
/// # use bevy_ecs::entity::{EntityMapper, Entity, MapEntities};
///
/// #[derive(Serialize, Deserialize, Clone)]
/// struct MyMessage(Entity);
///
/// impl MapEntities for MyMessage {
///    fn map_entities<M: EntityMapper>(&mut self, entity_map: &mut M) {
///        self.0 = entity_map.get_mapped(self.0);
///    }
/// }
///
/// fn add_messages(app: &mut App) {
///   app.register_message::<MyMessage>()
///       .add_map_entities();
/// }
/// ```
#[derive(Debug, Default, Clone, Resource, TypePath)]
pub struct MessageRegistry {
    /// Each message, at the index of its net id.
    messages: Vec<RegisteredMessage>,
    kind_map: TypeMapper<MessageKind>,
    /// The component ids of every typed receiver, listed when the registry is sealed and shared
    /// with each disconnection that removes them.
    receive_component_ids: Arc<[ComponentId]>,
    seal: RegistrySeal,
}

/// The context serialize fn of a message or event that maps the entities it holds.
pub(crate) fn mapped_context_serialize<M: MapEntities + Clone>(
    mapper: &SendMapView<'_>,
    message: &M,
    writer: &mut Writer,
    serialize_fn: SerializeFn<M>,
) {
    // A view that writes each entity as it is maps nothing, so the message is not cloned.
    if matches!(mapper, SendMapView::AsIs) {
        serialize_fn(message, writer);
        return;
    }
    let mut message = message.clone();
    // The view only exposes shared reads, so user mapping code runs without
    // requiring exclusive access to the entity map.
    let mut view = *mapper;
    message.map_entities(&mut view);
    serialize_fn(&message, writer);
}

/// The context deserialize fn of a message or event that maps the entities it holds.
pub(crate) fn mapped_context_deserialize<M: MapEntities>(
    mapper: &ReceiveMapView<'_>,
    reader: &mut Reader,
    deserialize_fn: DeserializeFn<M>,
) -> core::result::Result<M, MessageError> {
    let mut message = deserialize_fn(reader)?;
    // The view only exposes shared reads, so user mapping code runs without
    // requiring exclusive access to the entity map.
    let mut view = *mapper;
    message.map_entities(&mut view);
    Ok(message)
}

impl MessageRegistry {
    pub(crate) fn register<M: Message>(
        &mut self,
        mode: MessageModeFns,
        serialize: ContextSerializeFns<M>,
        deserialize: ContextDeserializeFns<M>,
    ) {
        self.open();
        trace!("Registering message: {}", DebugName::type_name::<M>());
        assert!(
            !self.is_registered::<M>(),
            "message type {} is already registered",
            DebugName::type_name::<M>()
        );
        self.kind_map.add::<M>();
        self.messages.push(RegisteredMessage {
            mode,
            serialize_fns: ErasedSerializeFns::new::<M>(serialize, deserialize),
            directions: Directions::default(),
        });
    }

    pub(crate) fn is_registered<M: 'static>(&self) -> bool {
        self.net_id(MessageKind::of::<M>()).is_some()
    }

    /// The net id of `kind`, if it is registered.
    pub(crate) fn net_id(&self, kind: MessageKind) -> Option<NetId> {
        self.kind_map.net_id(&kind).copied()
    }

    /// The registered message `net_id`, if one is: a received net id is the peer's to choose.
    pub(crate) fn get(&self, net_id: NetId) -> Option<&RegisteredMessage> {
        self.messages.get(usize::from(net_id.get()))
    }

    /// # Panics
    ///
    /// When `net_id` is not registered: a net id a link holds came from the registry.
    pub(crate) fn message(&self, net_id: NetId) -> &RegisteredMessage {
        &self.messages[usize::from(net_id.get())]
    }

    /// The type name of the message `net_id`, or `"Unknown"`.
    pub(crate) fn name(&self, net_id: NetId) -> &'static str {
        self.kind_map.name_at(net_id).unwrap_or("Unknown")
    }

    /// Each registered message, in the order of its net id.
    pub(crate) fn messages(&self) -> core::slice::Iter<'_, RegisteredMessage> {
        self.messages.iter()
    }

    fn message_mut<M: 'static>(&mut self) -> &mut RegisteredMessage {
        let net_id = self
            .net_id(MessageKind::of::<M>())
            .expect("a message is registered before it is configured");
        &mut self.messages[usize::from(net_id.get())]
    }

    /// Widens the directions `M` may travel in.
    pub(crate) fn add_direction<M: 'static>(&mut self, directions: Directions) {
        self.open();
        let message = self.message_mut::<M>();
        message.directions = message.directions.union(directions);
    }

    pub(crate) fn add_map_entities<M: Clone + MapEntities + 'static>(
        &mut self,
        context_serialize: ContextSerializeFn<M>,
        context_deserialize: ContextDeserializeFn<M>,
    ) {
        self.open();
        self.message_mut::<M>()
            .serialize_fns
            .map_entities::<M>(context_serialize, context_deserialize);
    }

    /// Writes `message`'s net id, then `message`, into `writer`.
    pub(crate) fn serialize<M: Message>(
        &self,
        message: &M,
        writer: &mut Writer,
        entity_map: &SendMapView<'_>,
    ) {
        let net_id = self
            .net_id(MessageKind::of::<M>())
            .expect("a message is registered before it is sent");
        net_id.to_bytes(writer);
        self.message(net_id)
            .serialize_fns
            .serialize::<M>(message, writer, entity_map);
    }

    fn open(&self) {
        self.seal.assert_open("message registry");
    }

    /// Seals the registry, as `MessagePlugin::finish` builds the message systems from it. Its
    /// hash holds each message's type, the directions it travels in and whether it maps entities,
    /// in the order of its id.
    pub(crate) fn seal(&mut self) {
        let mut hasher = RegistryHasher::new(HASH_DOMAIN);
        for ((_, name), message) in self.kind_map.iter().zip(&self.messages) {
            hasher.name(name);
            hasher.write(&[
                u8::from(message.directions),
                u8::from(message.serialize_fns.maps_entities),
            ]);
        }
        self.seal.seal("message registry", hasher.finish());
        self.receive_component_ids = self
            .messages
            .iter()
            .filter_map(RegisteredMessage::receive_component_ids)
            .flatten()
            .collect();
    }

    /// The component ids of every typed receiver, once the registry is sealed.
    pub(crate) const fn receive_component_ids(&self) -> &Arc<[ComponentId]> {
        &self.receive_component_ids
    }

    /// The hash peers compare, once the registry is sealed.
    pub const fn hash(&self) -> Option<RegistryHash> {
        self.seal.hash()
    }
}

#[cfg(test)]
mod tests;
