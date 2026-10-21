use super::*;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::ClientCopies;
use crate::fixtures::steps::PROPAGATION_FRAMES;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::VisibilityFilter;
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use campfire_net_connection::NetworkTarget;
use campfire_net_connection::{ClientOf, Connected};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

use crate::room_id::RoomId;

use crate::replication_target::Replicate;

use bevy_replicon::prelude::{AuthMethod, RepliconSharedPlugin};
use bevy_replicon::server::ServerPlugin;

use bevy_state::app::StatesPlugin;

/// App with the rooms filter, without networking.
fn rooms_app() -> App {
    let mut app = App::new();
    app.add_plugins(StatesPlugin);
    app.init_resource::<bevy_time::Time>();
    app.add_plugins(RepliconSharedPlugin {
        auth_method: AuthMethod::None,
    });
    app.add_plugins(ServerPlugin::default());
    app.add_plugins(RoomPlugin);
    app
}

/// Reads the rooms filter bit straight from the sender's [`ClientVisibility`].
fn is_hidden(app: &App, sender: Entity, entity: Entity) -> bool {
    let registry = app.world().resource::<FilterRegistry>();
    let bit = registry.get_bit::<Rooms>().unwrap();
    let visibility = app.world().get::<ClientVisibility>(sender).unwrap();
    visibility.get(entity).contains(bit)
}

#[test]
fn the_room_allocator_gives_distinct_rising_ids() {
    let mut allocator = RoomAllocator::default();

    let first = allocator.allocate();
    let second = allocator.allocate();

    assert_eq!(usize::from(first), 0);
    assert_eq!(usize::from(second), 1);
}

#[test]
fn a_room_set_adds_removes_and_lists_its_rooms() {
    let room_a = RoomId(0);
    let room_b = RoomId(3);
    let mut rooms = Rooms::single(room_a);

    rooms.add_room(room_b);

    assert!(rooms.contains_room(room_a));
    assert!(rooms.contains_room(room_b));
    assert_eq!(rooms.rooms().collect::<Vec<_>>(), [room_a, room_b]);

    rooms.remove_room(room_a);

    assert!(!rooms.contains_room(room_a));
    assert!(rooms.contains_room(room_b));
    assert_eq!(rooms.rooms().collect::<Vec<_>>(), [room_b]);
}

#[test]
fn the_room_filter_needs_a_shared_room() {
    let sender = Entity::from_bits(1);
    let room_a = RoomId(0);
    let room_b = RoomId(1);
    let entity_rooms = Rooms::single(room_a);
    let client_rooms = Rooms::single(room_a);
    let other_client_rooms = Rooms::single(room_b);

    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));
    assert!(!entity_rooms.is_visible(sender, Some(&other_client_rooms)));
    assert!(!entity_rooms.is_visible(sender, None));
}

#[test]
fn the_room_visibility_follows_each_move_of_a_client_or_an_entity() {
    let sender = Entity::from_bits(1);
    let room_a = RoomId(0);
    let room_b = RoomId(1);
    let mut entity_rooms = Rooms::single(room_a);
    let mut client_rooms = Rooms::single(room_a);

    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));

    client_rooms.remove_room(room_a);
    client_rooms.add_room(room_b);
    assert!(!entity_rooms.is_visible(sender, Some(&client_rooms)));

    entity_rooms.remove_room(room_a);
    entity_rooms.add_room(room_b);
    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));
}

#[test]
fn rooms_visibility_survives_entity_move_when_client_is_in_both_rooms() {
    let sender = Entity::from_bits(1);
    let room_a = RoomId(0);
    let room_b = RoomId(1);
    let mut entity_rooms = Rooms::single(room_a);
    let mut client_rooms = Rooms::single(room_a);
    client_rooms.add_room(room_b);

    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));

    entity_rooms.add_room(room_b);
    entity_rooms.remove_room(room_a);
    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));
}

#[test]
fn rooms_visibility_survives_client_move_when_entity_is_in_both_rooms() {
    let sender = Entity::from_bits(1);
    let room_a = RoomId(0);
    let room_b = RoomId(1);
    let mut entity_rooms = Rooms::single(room_a);
    let mut client_rooms = Rooms::single(room_a);
    entity_rooms.add_room(room_b);

    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));

    client_rooms.add_room(room_b);
    client_rooms.remove_room(room_a);
    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));
}

#[test]
fn the_room_visibility_goes_with_the_last_shared_room() {
    let sender = Entity::from_bits(1);
    let room_a = RoomId(0);
    let room_b = RoomId(1);
    let mut entity_rooms = Rooms::single(room_a);
    entity_rooms.add_room(room_b);
    let mut client_rooms = Rooms::single(room_a);
    client_rooms.add_room(room_b);

    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));

    entity_rooms.remove_room(room_a);
    assert!(entity_rooms.is_visible(sender, Some(&client_rooms)));

    entity_rooms.remove_room(room_b);
    assert!(!entity_rooms.is_visible(sender, Some(&client_rooms)));
}

#[test]
fn the_room_plugin_adds_the_allocator() {
    let mut app = App::new();

    app.add_plugins(RoomPlugin);

    assert!(app.world().contains_resource::<RoomAllocator>());
}

/// A link that replication admits sees the room-filtered entities that exist already exactly
/// when it shares a room with them, also when its rooms came before its `ClientVisibility`.
#[test]
fn a_new_client_evaluates_preexisting_room_entities() {
    let mut app = rooms_app();
    let room_a = RoomId(0);
    let room_b = RoomId(1);
    // entity exists before the link is admitted into replication
    let entity = app.world_mut().spawn(Rooms::single(room_a)).id();

    // link already carries its rooms when ClientVisibility arrives (as when
    // client Rooms are inserted in `On<Add<Connected>>`)
    let stranger = app.world_mut().spawn(Rooms::single(room_b)).id();
    app.world_mut()
        .entity_mut(stranger)
        .insert(ClientVisibility::default());
    assert!(is_hidden(&app, stranger, entity));

    // ...while a link in the same room sees the entity
    let member = app.world_mut().spawn(Rooms::single(room_a)).id();
    app.world_mut()
        .entity_mut(member)
        .insert(ClientVisibility::default());
    assert!(!is_hidden(&app, member, entity));

    // ...and a link without rooms yet sees nothing room-filtered
    let bare = app.world_mut().spawn_empty().id();
    app.world_mut()
        .entity_mut(bare)
        .insert(ClientVisibility::default());
    assert!(is_hidden(&app, bare, entity));
}

/// When client 2 connects:
/// - existing entities in rooms the new client is not in are not replicated to it
///
/// The new client's `Rooms` are already on its link when replicon inserts `ClientVisibility`.
#[test]
fn a_new_client_receives_only_the_entities_in_its_rooms() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

    let room_a = stepper
        .server_world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_b = stepper
        .server_world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();

    // client 0 shares room A with the entity
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_world_mut()
        .entity_mut(client_of_0_entity)
        .insert(Rooms::single(room_a));

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            Rooms::single(room_a),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    stepper
        .client_copy(0, server_entity)
        .expect("entity is not present in room-sharing client entity map");

    // second client connects; it joins room B as its link connects, before the link is
    // admitted into replication
    stepper.server_app_mut().add_observer(
        move |connected: On<'_, '_, Add<Connected>>,
              links: Query<'_, '_, (), (With<ClientOf>, Without<Rooms>)>,
              mut commands: Commands<'_, '_>| {
            if links.contains(connected.entity) {
                commands
                    .entity(connected.entity)
                    .insert(Rooms::single(room_b));
            }
        },
    );
    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(PROPAGATION_FRAMES);
    assert_eq!(
        stepper.client_of(1).get::<Rooms>(),
        Some(&Rooms::single(room_b))
    );

    // make sure the entity is not replicated to the newly connected client
    assert!(
        stepper.client_copy(1, server_entity).is_none(),
        "entity is present in new client entity map despite rooms mismatch"
    );
}
