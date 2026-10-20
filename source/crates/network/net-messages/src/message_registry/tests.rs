use super::*;
use crate::app_message_ext::AppMessageExt;
use crate::entity_check::EntityCheck;
use crate::message_plugin::MessagePlugin;
use crate::receive_entity_map::ReceiveEntityMap;
use crate::send_entity_map::SendEntityMap;
use bevy_ecs::entity::Entity;

use bevy_app::App;

use bevy_reflect::Reflect;

use crate::app_message_ext::register_message_with;
use crate::serialize_fns::SerializeFns;

use serde::Serialize;

impl MessageRegistry {
    pub(crate) fn deserialize<M: Message>(
        &self,
        reader: &mut Reader,
        entity_map: &ReceiveMapView<'_>,
    ) -> Result<M, MessageError> {
        let net_id = NetId::from_bytes(reader)?;
        let erased_fns = &self
            .get(net_id)
            .ok_or(MessageError::UnrecognizedNetId(net_id))?
            .serialize_fns;
        erased_fns.deserialize::<M>(reader, entity_map)
    }
}
use crate::app_event_ext::AppEventExt;
use crate::remote_entity_map::RemoteEntityMap;
use bevy_ecs::entity::EntityMapper;
use bevy_ecs::event::Event;
use campfire_net_bytes::ReadInteger;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::WriteInteger;
use serde::Deserialize;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Reflect)]
pub(super) struct Message1(pub f32);

/// Message where we provide our own serialization/deserialization functions
#[derive(Debug, PartialEq, Clone, Reflect)]
pub(super) struct Message2(pub f32);

pub(crate) fn serialize_message2(data: &Message2, writer: &mut Writer) {
    writer.write_u32(data.0.to_bits());
}

pub(crate) fn deserialize_message2(
    reader: &mut Reader,
) -> core::result::Result<Message2, MessageError> {
    let data = f32::from_bits(reader.read_u32().map_err(SerializationError::from)?);
    Ok(Message2(data))
}

/// Message where we provide our own serialization/deserialization functions
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Reflect)]
pub(super) struct Message3(pub Entity);

#[derive(Debug, Event, Serialize, Deserialize)]
struct EventMessage;

impl MapEntities for Message3 {
    fn map_entities<M: EntityMapper>(&mut self, entity_map: &mut M) {
        self.0 = entity_map.get_mapped(self.0);
    }
}

#[test]
#[should_panic(expected = "is already registered")]
fn a_type_registers_as_a_message_or_as_an_event_and_not_both() {
    let mut app = App::new();
    app.register_message::<EventMessage>();
    app.register_event::<EventMessage>();
}

/// A server that replicates no entity.
#[derive(Debug)]
struct Unreplicated;

impl EntityCheck for Unreplicated {
    fn holds(&self, _: Entity) -> bool {
        false
    }
}

/// A view of the pairs of `empty`, none, for a message that names no entity.
const fn unmapped(empty: &ReceiveEntityMap) -> ReceiveMapView<'_> {
    ReceiveMapView::Lookup {
        shared: None,
        local: empty,
    }
}

#[test]
fn a_message_round_trips() {
    let mut app = App::new();
    app.register_message::<Message1>();
    let registry = app.world().resource::<MessageRegistry>();

    let message = Message1(1.0);
    let mut writer = Writer::default();
    registry.serialize(&message, &mut writer, &SendMapView::AsIs);
    let data = writer.into_bytes();

    let mut reader = Reader::from(data);
    let read = registry
        .deserialize(&mut reader, &unmapped(&ReceiveEntityMap::default()))
        .unwrap();
    assert_eq!(message, read);
}

#[test]
fn a_message_round_trips_through_custom_fns() {
    let mut app = App::new();
    register_message_with::<Message2>(
        &mut app,
        SerializeFns {
            serialize: serialize_message2,
            deserialize: deserialize_message2,
        },
    );
    let registry = app.world().resource::<MessageRegistry>();

    let message = Message2(1.0);
    let mut writer = Writer::default();
    registry.serialize(&message, &mut writer, &SendMapView::AsIs);
    let data = writer.into_bytes();

    let mut reader = Reader::from(data);
    let read = registry
        .deserialize(&mut reader, &unmapped(&ReceiveEntityMap::default()))
        .unwrap();
    assert_eq!(message, read);
}

/// A client writes its entity 1 as the server's 2, which the server reads as its own, as it is the
/// link's; the server writes 2 as is, which the client reads back as its 1.
#[test]
fn a_message_maps_its_entities_on_the_client() {
    let mut app = App::new();
    app.register_message::<Message3>().add_map_entities();
    let registry = app.world().resource::<MessageRegistry>();
    let client = Entity::from_bits(1);
    let server = Entity::from_bits(2);
    let mut client_map = RemoteEntityMap::default();
    client_map.insert(server, client);

    let mut writer = Writer::default();
    let client_send = SendMapView::Lookup {
        shared: None,
        local: client_map.local_to_remote(),
    };
    registry.serialize(&Message3(client), &mut writer, &client_send);
    let mut reader = Reader::from(writer.take_written());
    let mut known = SendEntityMap::default();
    known.insert(server, client);
    let server_receive = ReceiveMapView::Checked {
        known: &known,
        replicated: &Unreplicated,
    };
    let read = registry
        .deserialize::<Message3>(&mut reader, &server_receive)
        .unwrap();
    assert_eq!(read.0, server);

    registry.serialize(&Message3(server), &mut writer, &SendMapView::AsIs);
    let mut reader = Reader::from(writer.take_written());
    let client_receive = ReceiveMapView::Lookup {
        shared: None,
        local: client_map.remote_to_local(),
    };
    let read = registry
        .deserialize::<Message3>(&mut reader, &client_receive)
        .unwrap();
    assert_eq!(read.0, client);
}

fn sealed_hash(build: impl FnOnce(&mut App)) -> RegistryHash {
    let mut app = App::new();
    build(&mut app);
    let mut registry = app.world_mut().resource_mut::<MessageRegistry>();
    registry.seal();
    registry.hash().unwrap()
}

/// The hash holds each message's directions and whether it maps entities, and not the order of
/// the calls that set them: one direction and then the other equals both at once.
#[test]
fn directions_and_entity_mapping_enter_the_hash() {
    let plain = sealed_hash(|app| {
        app.register_message::<Message3>();
    });
    let to_client = sealed_hash(|app| {
        app.register_message::<Message3>()
            .add_direction(LinkDirection::ServerToClient);
    });
    let both = sealed_hash(|app| {
        app.register_message::<Message3>()
            .add_direction(LinkDirections::BOTH);
    });
    let both_in_two = sealed_hash(|app| {
        app.register_message::<Message3>()
            .add_direction(LinkDirection::ServerToClient)
            .add_direction(LinkDirection::ClientToServer);
    });
    let mapped = sealed_hash(|app| {
        app.register_message::<Message3>().add_map_entities();
    });
    let hashes = [plain, to_client, both, mapped];
    for (i, hash) in hashes.iter().enumerate() {
        for other in &hashes[i + 1..] {
            assert_ne!(hash, other);
        }
    }
    assert_eq!(both, both_in_two);
}

/// `MessagePlugin::finish` seals the registry as it builds the message systems from it, so a
/// registration after it, or a direction added after it, panics.
#[test]
#[should_panic(expected = "the message registry takes a registration after its plugin finished")]
fn a_registration_after_the_message_plugin_finished_panics() {
    let mut app = App::new();
    app.add_plugins(MessagePlugin);
    app.finish();
    app.register_message::<Message1>();
}

#[test]
#[should_panic(expected = "the message registry takes a registration after its plugin finished")]
fn a_direction_after_the_message_plugin_finished_panics() {
    let mut app = App::new();
    app.add_plugins(MessagePlugin);
    app.register_message::<Message1>();
    app.finish();
    app.world_mut()
        .resource_mut::<MessageRegistry>()
        .add_direction::<Message1>(LinkDirections::BOTH);
}

/// A message whose clones a counter counts.
#[derive(Debug)]
struct Counted<'a>(&'a core::cell::Cell<u32>);

impl Clone for Counted<'_> {
    fn clone(&self) -> Self {
        self.0.set(self.0.get() + 1);
        Self(self.0)
    }
}

impl MapEntities for Counted<'_> {
    fn map_entities<E: EntityMapper>(&mut self, _: &mut E) {}
}

/// A view that writes entities as they are maps nothing, so the message is not cloned; a lookup
/// view maps a clone of it.
#[test]
fn only_a_mapping_view_clones_the_message() {
    let clones = core::cell::Cell::new(0);
    let message = Counted(&clones);
    let mut writer = Writer::default();
    let serialize: SerializeFn<Counted<'_>> = |_, _| {};
    mapped_cx_serialize(&SendMapView::AsIs, &message, &mut writer, serialize);
    assert_eq!(clones.get(), 0);
    let local = SendEntityMap::default();
    let lookup = SendMapView::Lookup {
        shared: None,
        local: &local,
    };
    mapped_cx_serialize(&lookup, &message, &mut writer, serialize);
    assert_eq!(clones.get(), 1);
}
