use super::*;
use crate::fixtures::protocol::*;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::inheritance_plugin::InheritancePlugin;
use crate::replication_target::Replicate;
use crate::visibility_ext::VisibilityExt;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::NetworkTarget;
use campfire_net_despawn::DespawnReplicated;
use campfire_net_messages::MessageManager;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;
use tracing::info;

use bevy_replicon::server::visibility::registry::FilterRegistry;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;

/// App with visibility inheritance and one fake sender, without networking.
fn visibility_app() -> (App, Entity) {
    let mut app = App::new();
    app.init_resource::<FilterRegistry>();
    app.init_resource::<ReplicationRegistry>();
    app.add_plugins((NetworkVisibilityPlugin, InheritancePlugin));
    let sender = app.world_mut().spawn(ClientVisibility::default()).id();
    (app, sender)
}

/// Reads effective visibility straight from the sender's
/// [`ClientVisibility`]: hidden iff the manual despawn-on-hide bit is set.
fn hidden(app: &App, sender: Entity, entity: Entity) -> bool {
    let bits = app.world().resource::<VisibilityBits>();
    let visibility = app.world().get::<ClientVisibility>(sender).unwrap();
    visibility.get(entity).contains(bits.while_visible)
}

fn is_overridden(app: &App, entity: Entity) -> bool {
    app.world()
        .get::<Overridden<ManualVisibility>>(entity)
        .is_some()
}

#[test]
fn members_follow_visibility() {
    let (mut app, sender) = visibility_app();
    let root = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn(ReplicateLike { root }).id();

    app.world_mut().lose_visibility(root, sender);
    assert!(hidden(&app, sender, root));
    assert!(hidden(&app, sender, child));

    app.world_mut().gain_visibility(root, sender);
    assert!(!hidden(&app, sender, root));
    assert!(!hidden(&app, sender, child));
    // A hierarchy write never marks.
    assert!(!is_overridden(&app, root));
    assert!(!is_overridden(&app, child));
}

/// Visibility set for a link that replication has not admitted yet applies, on the members that
/// follow too, once the link is: the last write for each entity wins.
#[test]
fn visibility_set_before_admission_applies_on_admission() {
    let (mut app, _) = visibility_app();
    let sender = app.world_mut().spawn_empty().id();
    let root = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn(ReplicateLike { root }).id();
    let other = app.world_mut().spawn_empty().id();

    app.world_mut().lose_visibility(root, sender);
    app.world_mut().lose_visibility(other, sender);
    app.world_mut().gain_visibility(other, sender);
    app.world_mut()
        .entity_mut(sender)
        .insert(ClientVisibility::default());
    app.world_mut().flush();

    assert!(hidden(&app, sender, root));
    assert!(hidden(&app, sender, child));
    assert!(!hidden(&app, sender, other));
    assert!(app.world().get::<PendingVisibility>(sender).is_none());
}

#[test]
fn direct_member_write_overrides_and_sticks() {
    let (mut app, sender) = visibility_app();
    let root = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn(ReplicateLike { root }).id();

    // Hide the child directly: later root pushes must skip it.
    app.world_mut().lose_visibility(child, sender);
    assert!(!hidden(&app, sender, root));
    assert!(hidden(&app, sender, child));
    assert!(is_overridden(&app, child));

    app.world_mut().gain_visibility(root, sender);
    assert!(!hidden(&app, sender, root));
    assert!(
        hidden(&app, sender, child),
        "root push should skip the overridden child"
    );

    // Removing the marker follows the root's (visible) state again.
    app.world_mut()
        .entity_mut(child)
        .remove::<Overridden<ManualVisibility>>();
    assert!(!hidden(&app, sender, child));
    assert!(!is_overridden(&app, child));

    // ...and the child follows the root again afterwards.
    app.world_mut().lose_visibility(root, sender);
    assert!(hidden(&app, sender, child));
}

#[test]
fn overridden_child_replicates_while_root_hidden() {
    let (mut app, sender) = visibility_app();
    let root = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn(ReplicateLike { root }).id();

    app.world_mut().lose_visibility(root, sender);
    app.world_mut().gain_visibility(child, sender);

    assert!(hidden(&app, sender, root));
    assert!(!hidden(&app, sender, child));
    assert!(is_overridden(&app, child));
}

#[test]
fn direct_root_write_does_not_mark() {
    let (mut app, sender) = visibility_app();
    let root = app.world_mut().spawn_empty().id();
    let standalone = app.world_mut().spawn_empty().id();

    app.world_mut().lose_visibility(root, sender);
    app.world_mut().lose_visibility(standalone, sender);

    assert!(!is_overridden(&app, root));
    assert!(!is_overridden(&app, standalone));
}

#[test]
fn gaining_visibility_spawns_the_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    stepper.frame_step(2);
    stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be auto-visible after spawn");

    // lose visibility
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_entity, client_of_0_entity);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none(),
        "entity should not be visible after lose_visibility"
    );

    // gain visibility again
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_entity, client_of_0_entity);
    stepper.frame_step(2);
    stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be visible again after gain_visibility");
}

#[test]
fn losing_visibility_despawns_the_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let sender = stepper.client_of_entity(0);
    stepper.frame_step(2);
    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();

    // lose visibility: a Despawn message should be sent
    stepper
        .server_app_mut()
        .world_mut()
        .lose_visibility(server_entity, sender);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none()
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_err()
    );

    // gain visibility: a spawn message should be sent again
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_entity, sender);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_some()
    );
}

#[test]
fn losing_retained_visibility_keeps_the_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompSimple(1.0)))
        .id();
    let sender = stepper.client_of_entity(0);
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompSimple>(client_entity),
        Some(&CompSimple(1.0))
    );

    // Retain the existing remote entity but stop sending updates.
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility_retained(server_entity, sender);
    stepper.frame_step(2);

    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied(),
        Some(client_entity),
        "retained visibility should preserve the remote entity mapping"
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .insert(CompSimple(2.0));
    stepper.frame_step(2);
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompSimple>(client_entity),
        Some(&CompSimple(1.0)),
        "retained hidden entities should not receive mutations"
    );

    stepper
        .server_app_mut()
        .world_mut()
        .gain_visibility(server_entity, sender);
    stepper.frame_step(2);
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied(),
        Some(client_entity),
        "gaining visibility should reuse the retained remote entity"
    );
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompSimple>(client_entity),
        Some(&CompSimple(2.0)),
        "gaining visibility should send the latest component value"
    );
}

#[test]
fn retained_visibility_before_the_first_replication_sends_nothing() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let sender = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .lose_visibility_retained(server_entity, sender);
    stepper.frame_step(2);

    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none(),
        "AfterFirstVisibility should not spawn an entity that was never visible"
    );

    stepper
        .server_app_mut()
        .world_mut()
        .gain_visibility(server_entity, sender);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_some(),
        "the entity should spawn when it becomes visible for the first time"
    );
}

#[test]
fn always_present_visibility_spawns_a_hidden_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompSimple(1.0)))
        .id();
    let sender = stepper.client_of_entity(0);

    // AlwaysPresent sends the initial state even when the entity starts hidden.
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility_always_present(server_entity, sender);
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("AlwaysPresent should spawn an entity that starts hidden");
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompSimple>(client_entity),
        Some(&CompSimple(1.0))
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .insert(CompSimple(2.0));
    stepper.frame_step(2);
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompSimple>(client_entity),
        Some(&CompSimple(1.0)),
        "AlwaysPresent entities should stop receiving updates while hidden"
    );

    stepper
        .server_app_mut()
        .world_mut()
        .gain_visibility(server_entity, sender);
    stepper.frame_step(2);
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied(),
        Some(client_entity),
        "gaining visibility should reuse the always-present remote entity"
    );
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompSimple>(client_entity),
        Some(&CompSimple(2.0)),
        "gaining visibility should send the latest component value"
    );

    // Visibility lifetime must not suppress a true authoritative despawn.
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility_always_present(server_entity, sender);
    stepper.frame_step(2);
    DespawnReplicated.apply(
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_entity),
    );
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_err(),
        "an authoritative despawn should override AlwaysPresent visibility"
    );
}

#[test]
fn a_despawn_reaches_a_retained_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let sender = stepper.client_of_entity(0);
    stepper.frame_step(2);
    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();

    stepper
        .server_app_mut()
        .world_mut()
        .lose_visibility_retained(server_entity, sender);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_ok(),
        "visibility loss should retain the remote entity"
    );

    DespawnReplicated.apply(
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_entity),
    );
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_err(),
        "an authoritative despawn should override retained visibility"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none()
    );
}

#[test]
fn removing_replicate_suppresses_the_despawn_of_a_retained_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let sender = stepper.client_of_entity(0);
    stepper.frame_step(2);
    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();

    stepper
        .server_app_mut()
        .world_mut()
        .lose_visibility_retained(server_entity, sender);
    stepper.frame_step(2);

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .remove::<Replicate>();
    DespawnReplicated.apply(
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_entity),
    );
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_ok(),
        "removing Replicate before despawning should keep the retained remote entity"
    );
}

/// https://github.com/cBournhonesque/lightyear/issues/637
/// Test that if an entity with NetworkVisibility is despawned, the DespawnMessage
/// is only sent to clients that have visibility on it.
#[test]
fn a_despawn_reaches_only_the_clients_that_see_the_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    let server_entity_0 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_entity_0, client_of_0_entity);

    let server_entity_1 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    let client_of_1_entity = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_entity_1, client_of_1_entity);

    stepper.frame_step(2);
    let client_entity_0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity_0)
        .copied()
        .unwrap();
    let client_entity_1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity_1)
        .copied()
        .unwrap();

    // update the entity_map on the second connection to re-use the same entity
    // as client 0, so that we can check if the second connection also receives a despawn
    // when we despawn the entity for the first connection.
    //
    // i.e. a DespawnMessage for server_entity_0 send to all clients will also despawn
    // client_entity_1 This only works because the replication group is the same for both entities.
    stepper
        .client_mut(1)
        .get_mut::<MessageManager>()
        .unwrap()
        .entity_mapper
        .insert(server_entity_0, client_entity_1);

    DespawnReplicated.apply(
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_entity_0),
    );
    stepper.frame_step(2);

    // client entity 0 has been despawned
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity_0)
            .is_err()
    );

    // client entity 1 has not been despawned (because connection 1 did not have visibility on
    // server_entity_0)
    assert!(
        stepper
            .client_app(1)
            .world()
            .get_entity(client_entity_1)
            .is_ok()
    );
}

/// Test that there is no logspam when we despawn an entity with NetworkVisibility
/// but that is not visible to a client
#[test]
fn despawning_an_entity_hidden_from_a_client_logs_nothing() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());
    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
        .id();
    let _server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ChildOf(server_parent),
        ))
        .id();

    stepper.frame_step(1);
    info!("Server despawning parent that is not visible to the client");
    DespawnReplicated.apply(
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_parent),
    );
    stepper.frame_step(10);
}

/// https://github.com/cBournhonesque/lightyear/issues/1347
/// Repeated `lose_visibility` calls should not disturb the entity's replication target.
#[test]
fn repeated_lose_visibility_keeps_the_replication_target() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    stepper.frame_step(2);
    stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be auto-visible after spawn");

    // lose visibility
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_entity, client_of_0_entity);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none(),
        "entity should not be visible after lose_visibility"
    );

    // lose visibility again (idempotent): should not cause issues
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_entity, client_of_0_entity);
    stepper.frame_step(2);

    // gain visibility: we should be able to replicate
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_entity, client_of_0_entity);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_some(),
        "entity should be visible again after gain_visibility"
    );
}

/// Test that visibility overrides are NOT reset when ReplicationTarget is replaced.
///
/// Scenario: server spawns entity visible to clients 1 and 2, then lose_visibility for client 2,
/// then re-insert Replicate targeting all clients. Client 2 should still not see the entity.
#[test]
fn visibility_overrides_survive_a_replication_target_change() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    stepper.frame_step(2);

    // both clients should see the entity
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_some(),
        "client 0 should see the entity"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_some(),
        "client 1 should see the entity"
    );

    // lose visibility for client 1 (second client, index 1)
    let sender_1 = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .lose_visibility(server_entity, sender_1);
    stepper.frame_step(2);

    // client 1 should no longer see the entity
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none(),
        "client 1 should not see the entity after lose_visibility"
    );

    // re-insert Replicate targeting all clients (triggers on_discard + on_insert)
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .insert(Replicate::to_clients(NetworkTarget::All));
    stepper.frame_step(2);

    // client 0 should still see the entity
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_some(),
        "client 0 should still see the entity after ReplicationTarget change"
    );
    // client 1 should still NOT see the entity (visibility override persists)
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .copied()
            .is_none(),
        "client 1 should still not see the entity after ReplicationTarget change"
    );
}
