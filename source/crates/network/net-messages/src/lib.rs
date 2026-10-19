//! This crate provides a [`MessagePlugin`] to handle sending and receiving messages over the
//! network.
//!
//! A [`Message`] is simply any type that can be (de)serialized.
//!
//! You can add the [`MessageSender<M>`](crate::message_sender::MessageSender) or
//! [`MessageReceiver<M>`](crate::message_receiver::MessageReceiver) components to your Link entity
//! to enable sending and receiving messages of type `M`.
//!
//! A host client is one entity that is both the client and the server's link to it. Its
//! [`MessageSender<M>`], [`MessageReceiver<M>`] and [`EventSender<M>`] are the server's link's;
//! its client systems use [`HostMessageSender<M>`], [`HostMessageReceiver<M>`] and
//! [`HostEventSender<M>`].
//!
//! The crate also provides a [`MessageManager`] component that manages the process of sending and
//! receiving messages for an entity. It stores a [`RemoteEntityMap`] that holds a mapping between
//! the local and remote entities.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod app_event_ext;
mod app_message_ext;
#[cfg(feature = "internals")]
pub mod bench;
mod endpoint_side;
mod entity_check;
mod event_registration;
mod event_sender;
#[cfg(feature = "internals")]
mod fixtures;
mod host_event_sender;
mod host_message_sender;
mod message;
mod message_error;
mod message_kind;
mod message_manager;
mod message_plugin;
mod message_receive_context;
mod message_receiver;
mod message_registration;
mod message_registry;
mod message_sender;
mod message_systems;
mod multi_message_sender;
mod receive_entity_map;
mod receive_map_view;
mod received_message;
mod remote_entity_map;
mod remote_event;
mod replicon_map_param;
mod send_entity_map;
mod send_map_view;
mod send_queue;
mod serialize_fns;
mod wire_entity;

pub use crate::app_event_ext::AppEventExt;
pub use crate::app_message_ext::AppMessageExt;
pub use crate::entity_check::EntityCheck;
pub use crate::event_registration::EventRegistration;
pub use crate::event_sender::EventSender;
pub use crate::host_event_sender::HostEventSender;
pub use crate::host_message_sender::HostMessageSender;
pub use crate::message::Message;
pub use crate::message_error::MessageError;
pub use crate::message_kind::MessageKind;
pub use crate::message_manager::MessageManager;
pub use crate::message_plugin::MessagePlugin;
pub use crate::message_receive_context::ReceivedMessageHeader;
pub use crate::message_receiver::HostMessageReceiver;
pub use crate::message_receiver::MessageReceiver;
pub use crate::message_registration::MessageRegistration;
pub use crate::message_registry::MessageRegistry;
pub use crate::message_sender::MessageSender;
pub use crate::message_systems::MessageSystems;
pub use crate::multi_message_sender::MultiMessageSender;
pub use crate::receive_entity_map::ReceiveEntityMap;
pub use crate::receive_map_view::ReceiveMapView;
pub use crate::received_message::ReceivedMessage;
pub use crate::remote_entity_map::RemoteEntityMap;
pub use crate::remote_event::RemoteEvent;
pub use crate::send_entity_map::SendEntityMap;
pub use crate::send_map_view::SendMapView;
pub use crate::send_queue::Priority;
pub use crate::wire_entity::WireEntity;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::host_received_messages::HostReceivedMessages;
    pub use crate::fixtures::messages_stack::MessagesStack;
    pub use crate::fixtures::protocol::{
        ClientMessage, EntityMessage, MappedEntityEvent, MessagesProtocolPlugin, SeqMessage,
        StringEvent, StringMessage,
    };
    pub use crate::fixtures::received_messages::ReceivedMessages;
}
