use bevy_app::{App, Plugin};
use bevy_ecs::entity::{Entity, MapEntities};
use bevy_ecs::event::{EntityEvent, Event};
use bevy_reflect::Reflect;
use campfire_net_core::{LinkDirection, LinkDirections};
use serde::{Deserialize, Serialize};

use crate::app_event_ext::AppEventExt;
use crate::app_message_ext::AppMessageExt;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Reflect)]
pub struct StringMessage(pub String);

/// A message only a client sends.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Reflect)]
pub struct ClientMessage(pub String);

/// A numbered message that names the channel it was sent on, so a receiver checks each channel's
/// guarantee.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Reflect)]
pub struct SeqMessage {
    pub channel: u8,
    pub seq: u32,
}

/// A message that names an entity, which each side maps as its side of the link does.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Reflect, MapEntities)]
pub struct EntityMessage(#[entities] pub Entity);

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Reflect, Event)]
pub struct StringEvent(pub String);

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Reflect, EntityEvent, MapEntities,
)]
pub struct MappedEntityEvent(
    #[event_target]
    #[entities]
    pub Entity,
);

/// The messages layer's test protocol: messages and events, both ways, and a message one way.
#[derive(Debug)]
pub struct MessagesProtocolPlugin;

impl Plugin for MessagesProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.register_message::<StringMessage>()
            .add_direction(LinkDirections::BOTH);
        app.register_message::<ClientMessage>()
            .add_direction(LinkDirection::ClientToServer);
        app.register_message::<SeqMessage>()
            .add_direction(LinkDirections::BOTH);
        app.register_message::<EntityMessage>()
            .add_map_entities()
            .add_direction(LinkDirections::BOTH);
        app.register_event::<StringEvent>()
            .add_direction(LinkDirections::BOTH);
        app.register_event::<MappedEntityEvent>()
            .add_map_entities()
            .add_direction(LinkDirections::BOTH);
    }
}
