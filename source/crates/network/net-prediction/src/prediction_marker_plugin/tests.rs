use crate::fixtures::prediction_stack::PredictionStack;
use bevy::prelude::*;
use bevy_replicon::prelude::Remote;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::RemoteId;
use campfire_net_connection::{ClientOf, Connected, NetworkTarget};
use campfire_net_interpolation::internals::*;
use campfire_net_replication::internals::*;
use campfire_net_replication::{
    Controlled, ControlledBy, ControlledLifetime, Interpolated, InterpolationTarget, Overridden,
    Predicted, PredictedSend, PredictionTarget, Replicate, RoomAllocator, Rooms,
};
use campfire_net_stepper::{Init, StepperSetup};
use campfire_net_stepper::{StepOrder, Stepper};
use tracing::info;

#[test]
fn late_join_client_gets_predicted_marker_for_prediction_target_all() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            CompA(1.0),
        ))
        .id();

    let server_entity_ref = stepper.server_app().world().entity(server_entity);
    assert!(server_entity_ref.contains::<PredictedSend>());
    assert!(
        !server_entity_ref.contains::<Predicted>(),
        "a prediction target must not mark the authoritative sender as predicted"
    );

    stepper.frame_step(3);

    let client_0_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(client_0_entity)
            .is_some(),
        "existing client should see the entity as predicted"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<PredictedSend>(client_0_entity)
            .is_none(),
        "the sender-only prediction marker must not be materialized on the receiver"
    );

    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(3);

    let client_1_entity = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("late-joining client should receive the entity");
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(client_1_entity)
            .is_some(),
        "late-joining client should see PredictionTarget::All entity as predicted"
    );
}

#[test]
fn the_removal_of_a_prediction_target_removes_the_receiver_marker() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();
    assert!(
        stepper
            .client_app(0)
            .world()
            .entity(client_entity)
            .contains::<Predicted>()
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .remove::<PredictionTarget>();
    stepper.frame_step(2);

    assert!(
        !stepper
            .client_app(0)
            .world()
            .entity(client_entity)
            .contains::<Predicted>()
    );
}

#[test]
fn a_client_that_joins_late_gets_the_latest_state_of_a_predicted_entity() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            CompA(0.0),
        ))
        .id();

    stepper.frame_step(2);

    for value in [1.0, 2.0, 3.0, 4.0] {
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_entity)
            .insert(CompA(value));
        stepper.frame_step(1);
    }

    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(3);

    let client_1_entity = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("late-joining client should receive the entity");
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(client_1_entity)
            .is_some(),
        "late-joining client should see the entity as predicted"
    );
    assert_eq!(
        stepper.client_app(1).world().get::<CompA>(client_1_entity),
        Some(&CompA(4.0)),
        "late-joining client should receive the latest component state"
    );
}

/// With 2 clients, PredictionTarget::to_clients(NetworkTarget::single(client_id)) should
/// only give the Predicted component to that specific client, not all clients.
#[test]
fn a_single_prediction_target_predicts_on_that_client_only() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;
    let client_1_id = stepper.client_of(1).get::<RemoteId>().unwrap().0;

    // server spawns entity for client 0: predicted for client 0, interpolated for client 1
    let entity_for_client_0 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_0_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_0_id])),
        ))
        .id();

    // server spawns entity for client 1: predicted for client 1, interpolated for client 0
    let entity_for_client_1 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_1_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_1_id])),
        ))
        .id();

    stepper.frame_step(4);

    // On client 0:
    let mm_0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client();
    let e0_on_client_0 = mm_0
        .get(&entity_for_client_0)
        .copied()
        .expect("entity_for_client_0 should be replicated to client 0");
    let e1_on_client_0 = mm_0
        .get(&entity_for_client_1)
        .copied()
        .expect("entity_for_client_1 should be replicated to client 0");

    // entity_for_client_0 should have Predicted on client 0
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e0_on_client_0)
            .is_some(),
        "entity_for_client_0 should have Predicted on client 0"
    );
    // entity_for_client_1 should NOT have Predicted on client 0
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e1_on_client_0)
            .is_none(),
        "entity_for_client_1 should NOT have Predicted on client 0 (should be Interpolated)"
    );
    // entity_for_client_1 should have Interpolated on client 0
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(e1_on_client_0)
            .is_some(),
        "entity_for_client_1 should have Interpolated on client 0"
    );
    // entity_for_client_0 should NOT have Interpolated on client 0
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(e0_on_client_0)
            .is_none(),
        "entity_for_client_0 should NOT have Interpolated on client 0"
    );

    // On client 1:
    let mm_1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client();
    let e0_on_client_1 = mm_1
        .get(&entity_for_client_0)
        .copied()
        .expect("entity_for_client_0 should be replicated to client 1");
    let e1_on_client_1 = mm_1
        .get(&entity_for_client_1)
        .copied()
        .expect("entity_for_client_1 should be replicated to client 1");

    // entity_for_client_1 should have Predicted on client 1
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(e1_on_client_1)
            .is_some(),
        "entity_for_client_1 should have Predicted on client 1"
    );
    // entity_for_client_0 should NOT have Predicted on client 1
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(e0_on_client_1)
            .is_none(),
        "entity_for_client_0 should NOT have Predicted on client 1 (should be Interpolated)"
    );
    // entity_for_client_0 should have Interpolated on client 1
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Interpolated>(e0_on_client_1)
            .is_some(),
        "entity_for_client_0 should have Interpolated on client 1"
    );
    // entity_for_client_1 should NOT have Interpolated on client 1
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Interpolated>(e1_on_client_1)
            .is_none(),
        "entity_for_client_1 should NOT have Interpolated on client 1"
    );

    // Count predicted entities per client - each should have exactly 1
    let predicted_count_client_0 = stepper
        .client_app_mut(0)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(0).world())
        .count();
    assert_eq!(
        predicted_count_client_0, 1,
        "Client 0 should have exactly 1 predicted entity, got {predicted_count_client_0}"
    );

    let predicted_count_client_1 = stepper
        .client_app_mut(1)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(1).world())
        .count();
    assert_eq!(
        predicted_count_client_1, 1,
        "Client 1 should have exactly 1 predicted entity, got {predicted_count_client_1}"
    );
}

/// Same as a_single_prediction_target_predicts_on_that_client_only but entities are
/// spawned sequentially (first entity replicated before second spawns), which
/// more closely mimics how the simple_box example works.
#[test]
fn sequential_spawns_predict_only_on_their_target_client() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;
    let client_1_id = stepper.client_of(1).get::<RemoteId>().unwrap().0;

    // server spawns entity for client 0 first
    let entity_for_client_0 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_0_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_0_id])),
        ))
        .id();

    // Let entity 0 be fully replicated before spawning entity 1
    stepper.frame_step(3);

    // Now spawn entity for client 1
    let entity_for_client_1 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_1_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_1_id])),
        ))
        .id();

    stepper.frame_step(3);

    // On client 0: should have exactly 1 predicted entity
    let predicted_count_client_0 = stepper
        .client_app_mut(0)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(0).world())
        .count();
    assert_eq!(
        predicted_count_client_0, 1,
        "Client 0 should have exactly 1 predicted entity, got {predicted_count_client_0}"
    );

    // On client 1: should have exactly 1 predicted entity
    let predicted_count_client_1 = stepper
        .client_app_mut(1)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(1).world())
        .count();
    assert_eq!(
        predicted_count_client_1, 1,
        "Client 1 should have exactly 1 predicted entity, got {predicted_count_client_1}"
    );

    // Verify correct assignment
    let mm_0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client();
    let e0_on_client_0 = mm_0.get(&entity_for_client_0).copied().unwrap();
    let e1_on_client_0 = mm_0.get(&entity_for_client_1).copied().unwrap();
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e0_on_client_0)
            .is_some()
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e1_on_client_0)
            .is_none()
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(e1_on_client_0)
            .is_some()
    );

    let mm_1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client();
    let e0_on_client_1 = mm_1.get(&entity_for_client_0).copied().unwrap();
    let e1_on_client_1 = mm_1.get(&entity_for_client_1).copied().unwrap();
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(e1_on_client_1)
            .is_some()
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(e0_on_client_1)
            .is_none()
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Interpolated>(e0_on_client_1)
            .is_some()
    );
}

/// Mimics the simple_box pattern: entities spawned via observer on Connected,
/// with ControlledBy + PredictionTarget/InterpolationTarget.
/// Each client sees exactly 1 Predicted entity (their own) and
/// the other client's entity as Interpolated.
#[test]
fn each_client_predicts_its_own_entity_and_interpolates_the_others() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    // Mimic handle_connected: for each connected client, spawn a player entity
    let client_of_0 = stepper.client_of_entity(0);
    let client_of_1 = stepper.client_of_entity(1);
    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;
    let client_1_id = stepper.client_of(1).get::<RemoteId>().unwrap().0;

    // Spawn entity for client 0 (like handle_connected would)
    let entity_for_0 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_0_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_0_id])),
            ControlledBy {
                link: client_of_0,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();
    info!(
        "Spawned entity_for_0: {:?} targeting client {:?}",
        entity_for_0, client_0_id
    );

    // Let it replicate
    stepper.frame_step(3);

    // Now spawn entity for client 1 (like a second client connecting later)
    let entity_for_1 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_1_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_1_id])),
            ControlledBy {
                link: client_of_1,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();
    info!(
        "Spawned entity_for_1: {:?} targeting client {:?}",
        entity_for_1, client_1_id
    );

    stepper.frame_step(3);

    // Check client 0
    let mm_0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client();
    let e0_on_c0 = mm_0
        .get(&entity_for_0)
        .copied()
        .expect("entity_for_0 not replicated to client 0");
    let e1_on_c0 = mm_0
        .get(&entity_for_1)
        .copied()
        .expect("entity_for_1 not replicated to client 0");

    let e0_predicted = stepper
        .client_app(0)
        .world()
        .get::<Predicted>(e0_on_c0)
        .is_some();
    let e0_interpolated = stepper
        .client_app(0)
        .world()
        .get::<Interpolated>(e0_on_c0)
        .is_some();
    let e0_controlled = stepper
        .client_app(0)
        .world()
        .get::<Controlled>(e0_on_c0)
        .is_some();
    let e1_predicted = stepper
        .client_app(0)
        .world()
        .get::<Predicted>(e1_on_c0)
        .is_some();
    let e1_interpolated = stepper
        .client_app(0)
        .world()
        .get::<Interpolated>(e1_on_c0)
        .is_some();
    let e1_controlled = stepper
        .client_app(0)
        .world()
        .get::<Controlled>(e1_on_c0)
        .is_some();

    info!(
        "Client 0: entity_for_0 ({:?}): predicted={}, interpolated={}, controlled={}",
        e0_on_c0, e0_predicted, e0_interpolated, e0_controlled
    );
    info!(
        "Client 0: entity_for_1 ({:?}): predicted={}, interpolated={}, controlled={}",
        e1_on_c0, e1_predicted, e1_interpolated, e1_controlled
    );

    assert!(e0_predicted, "Client 0's own entity should be Predicted");
    assert!(
        !e0_interpolated,
        "Client 0's own entity should NOT be Interpolated"
    );
    assert!(
        !e1_predicted,
        "Client 1's entity on client 0 should NOT be Predicted"
    );
    assert!(
        e1_interpolated,
        "Client 1's entity on client 0 should be Interpolated"
    );

    // Check client 1
    let mm_1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client();
    let e0_on_c1 = mm_1
        .get(&entity_for_0)
        .copied()
        .expect("entity_for_0 not replicated to client 1");
    let e1_on_c1 = mm_1
        .get(&entity_for_1)
        .copied()
        .expect("entity_for_1 not replicated to client 1");

    let e0_predicted_c1 = stepper
        .client_app(1)
        .world()
        .get::<Predicted>(e0_on_c1)
        .is_some();
    let e0_interpolated_c1 = stepper
        .client_app(1)
        .world()
        .get::<Interpolated>(e0_on_c1)
        .is_some();
    let e1_predicted_c1 = stepper
        .client_app(1)
        .world()
        .get::<Predicted>(e1_on_c1)
        .is_some();
    let e1_interpolated_c1 = stepper
        .client_app(1)
        .world()
        .get::<Interpolated>(e1_on_c1)
        .is_some();

    info!(
        "Client 1: entity_for_0 ({:?}): predicted={}, interpolated={}",
        e0_on_c1, e0_predicted_c1, e0_interpolated_c1
    );
    info!(
        "Client 1: entity_for_1 ({:?}): predicted={}, interpolated={}",
        e1_on_c1, e1_predicted_c1, e1_interpolated_c1
    );

    assert!(
        !e0_predicted_c1,
        "Client 0's entity on client 1 should NOT be Predicted"
    );
    assert!(
        e0_interpolated_c1,
        "Client 0's entity on client 1 should be Interpolated"
    );
    assert!(e1_predicted_c1, "Client 1's own entity should be Predicted");
    assert!(
        !e1_interpolated_c1,
        "Client 1's own entity should NOT be Interpolated"
    );

    // Count
    let predicted_count_0 = stepper
        .client_app_mut(0)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(0).world())
        .count();
    let predicted_count_1 = stepper
        .client_app_mut(1)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(1).world())
        .count();
    assert_eq!(
        predicted_count_0, 1,
        "Client 0 should have exactly 1 Predicted entity, got {predicted_count_0}"
    );
    assert_eq!(
        predicted_count_1, 1,
        "Client 1 should have exactly 1 Predicted entity, got {predicted_count_1}"
    );
}

/// Test prediction visibility with a late-joining client.
/// Client 0 connects first, entity spawned for it. Then client 1 joins.
/// Client 1 does NOT see client 0's entity as Predicted.
#[test]
fn a_late_client_does_not_predict_another_clients_entity() {
    // Start with 1 client
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;
    let client_of_0 = stepper.client_of_entity(0);

    // Spawn entity for client 0
    let entity_for_0 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_0_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_0_id])),
            ControlledBy {
                link: client_of_0,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();

    stepper.frame_step(3);

    // Verify client 0 sees the entity as Predicted
    let e0_on_c0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_0)
        .copied()
        .unwrap();
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e0_on_c0)
            .is_some(),
        "Client 0 should see its own entity as Predicted"
    );

    // Now client 1 connects (late join)
    stepper.add_remote_client();
    stepper.init();

    let client_1_id = stepper.client_of(1).get::<RemoteId>().unwrap().0;
    let client_of_1 = stepper.client_of_entity(1);

    // Spawn entity for client 1
    let entity_for_1 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_1_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_1_id])),
            ControlledBy {
                link: client_of_1,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();

    stepper.frame_step(3);

    // Check entity_for_0 on client 1 (late joiner)
    let e0_on_c1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_0)
        .copied()
        .expect("entity_for_0 should be replicated to client 1");
    let e1_on_c1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_1)
        .copied()
        .expect("entity_for_1 should be replicated to client 1");

    let e0_pred_c1 = stepper
        .client_app(1)
        .world()
        .get::<Predicted>(e0_on_c1)
        .is_some();
    let e0_interp_c1 = stepper
        .client_app(1)
        .world()
        .get::<Interpolated>(e0_on_c1)
        .is_some();
    let e1_pred_c1 = stepper
        .client_app(1)
        .world()
        .get::<Predicted>(e1_on_c1)
        .is_some();
    let e1_interp_c1 = stepper
        .client_app(1)
        .world()
        .get::<Interpolated>(e1_on_c1)
        .is_some();

    info!(
        "Late-join Client 1: entity_for_0: predicted={}, interpolated={}",
        e0_pred_c1, e0_interp_c1
    );
    info!(
        "Late-join Client 1: entity_for_1: predicted={}, interpolated={}",
        e1_pred_c1, e1_interp_c1
    );

    assert!(
        !e0_pred_c1,
        "Client 0's entity should NOT be Predicted on late-joining client 1"
    );
    assert!(
        e0_interp_c1,
        "Client 0's entity should be Interpolated on late-joining client 1"
    );
    assert!(e1_pred_c1, "Client 1's own entity should be Predicted");
    assert!(
        !e1_interp_c1,
        "Client 1's own entity should NOT be Interpolated"
    );

    // Also check entity_for_1 on client 0
    let e1_on_c0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_1)
        .copied()
        .expect("entity_for_1 should be replicated to client 0");

    let e1_pred_c0 = stepper
        .client_app(0)
        .world()
        .get::<Predicted>(e1_on_c0)
        .is_some();
    let e1_interp_c0 = stepper
        .client_app(0)
        .world()
        .get::<Interpolated>(e1_on_c0)
        .is_some();

    info!(
        "Client 0: entity_for_1: predicted={}, interpolated={}",
        e1_pred_c0, e1_interp_c0
    );

    assert!(
        !e1_pred_c0,
        "Client 1's entity should NOT be Predicted on client 0"
    );
    assert!(
        e1_interp_c0,
        "Client 1's entity should be Interpolated on client 0"
    );

    // Count predicted per client
    let pred_count_0 = stepper
        .client_app_mut(0)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(0).world())
        .count();
    let pred_count_1 = stepper
        .client_app_mut(1)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(1).world())
        .count();
    assert_eq!(
        pred_count_0, 1,
        "Client 0 should have exactly 1 Predicted, got {pred_count_0}"
    );
    assert_eq!(
        pred_count_1, 1,
        "Client 1 should have exactly 1 Predicted, got {pred_count_1}"
    );
}

/// Mimics the simple_box example most closely: entities spawned via observer
/// on Connected, with ControlledBy + PredictionTarget/InterpolationTarget.
/// Client 1 connects late (after client 0's entity is already replicated).
#[test]
fn observer_spawned_entities_predict_only_on_their_owner_after_a_late_join() {
    // Start with 1 client only
    let mut setup = StepperSetup::single();
    setup.init = Init::Deferred;
    let mut stepper = Stepper::new(PredictionStack, setup);

    // Register observer on the SERVER that spawns player entities when clients connect
    // (mimicking simple_box handle_connected)
    stepper.server_app_mut().add_observer(
        |trigger: On<'_, '_, Add<Connected>>,
         query: Query<'_, '_, &RemoteId, With<ClientOf>>,
         mut commands: Commands<'_, '_>| {
            let Ok(remote_id) = query.get(trigger.entity) else {
                return;
            };
            let client_id = remote_id.0;
            info!(
                "Observer: spawning player for client {:?} (client_of {:?})",
                client_id, trigger.entity
            );
            commands.spawn((
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::single(client_id)),
                InterpolationTarget::to_clients(NetworkTarget::all_except([client_id])),
                ControlledBy {
                    link: trigger.entity,
                    lifetime: ControlledLifetime::default(),
                },
                Name::from(format!("Player_{client_id:?}")),
            ));
        },
    );

    stepper.init();

    // Client 0 is now connected and should have triggered the observer
    stepper.frame_step(3);

    // Verify client 0 has 1 predicted entity (its own)
    let pred_count_0 = stepper
        .client_app_mut(0)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(0).world())
        .count();
    assert_eq!(
        pred_count_0, 1,
        "After first client connects, client 0 should have 1 Predicted entity, got {pred_count_0}"
    );

    // Now client 1 joins late
    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(3);

    // Each client should have exactly 1 predicted entity
    let pred_count_0 = stepper
        .client_app_mut(0)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(0).world())
        .count();
    let pred_count_1 = stepper
        .client_app_mut(1)
        .world_mut()
        .query_filtered::<Entity, With<Predicted>>()
        .iter(stepper.client_app(1).world())
        .count();

    info!("Client 0 predicted count: {pred_count_0}");
    info!("Client 1 predicted count: {pred_count_1}");

    // Log details for each client
    for client_idx in 0..stepper.client_count() {
        let world = stepper.client_app_mut(client_idx).world_mut();
        let predicted_entities: Vec<Entity> = world
            .query_filtered::<Entity, With<Predicted>>()
            .iter(world)
            .collect();
        let interpolated_entities: Vec<Entity> = world
            .query_filtered::<Entity, With<Interpolated>>()
            .iter(world)
            .collect();
        info!(
            "Client {}: predicted={:?}, interpolated={:?}",
            client_idx, predicted_entities, interpolated_entities
        );
    }

    assert_eq!(
        pred_count_0, 1,
        "Client 0 should have exactly 1 Predicted entity, got {pred_count_0}"
    );
    assert_eq!(
        pred_count_1, 1,
        "Client 1 should have exactly 1 Predicted entity, got {pred_count_1}"
    );
}

/// Re-inserting a Replicate component works as expected (doesn't
/// create duplicate entities)
/// https://github.com/cBournhonesque/lightyear/issues/1025
#[test]
fn reinserting_replicate_creates_no_duplicate_entity() {
    {
        let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

        let source_entity = spawn_on_server(
            &mut stepper,
            (Replicate::to_clients(NetworkTarget::All), CompA(1.0)),
        );
        stepper.frame_step(PROPAGATION_FRAMES);

        let mirrored_entity = client_copy(&stepper, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));

        on_server(&mut stepper, |world| {
            world
                .entity_mut(source_entity)
                .insert(Replicate::to_clients(NetworkTarget::All));
        });
        stepper.frame_step(PROPAGATION_FRAMES);

        let mapped_after_reinsert = client_copy(&stepper, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        assert_eq!(
            mapped_after_reinsert, mirrored_entity,
            "re-inserting Replicate should keep the same mapped entity"
        );

        let replicated_count = on_client(&mut stepper, |world| {
            world
                .query_filtered::<Entity, (With<CompA>, With<Remote>)>()
                .iter(world)
                .count()
        });
        assert_eq!(
            replicated_count, 1,
            "re-inserting Replicate should not create duplicate entities"
        );
    }
}

/// Predicted and Interpolated are correctly distributed
/// across clients. Each client's own entity should be Predicted, and other
/// clients' entities should be Interpolated.
#[test]
fn each_client_gets_predicted_for_its_own_entity_and_interpolated_for_others() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;
    let client_1_id = stepper.client_of(1).get::<RemoteId>().unwrap().0;
    let client_of_0 = stepper.client_of_entity(0);
    let client_of_1 = stepper.client_of_entity(1);

    // Spawn entity for client 0 (like simple_box's handle_connected observer)
    let entity_for_0 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_0_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_0_id])),
            ControlledBy {
                link: client_of_0,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();

    // Spawn entity for client 1
    let entity_for_1 = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_1_id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([client_1_id])),
            ControlledBy {
                link: client_of_1,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();

    // Step frames to replicate to clients
    stepper.frame_step(4);

    // Verify on client 0
    let e0_on_c0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_0)
        .copied()
        .unwrap();
    let e1_on_c0 = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_1)
        .copied()
        .unwrap();

    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e0_on_c0)
            .is_some(),
        "Client 0 should see entity_for_0 as Predicted"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(e0_on_c0)
            .is_none(),
        "Client 0 should NOT see entity_for_0 as Interpolated"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(e1_on_c0)
            .is_none(),
        "Client 0 should NOT see entity_for_1 as Predicted"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(e1_on_c0)
            .is_some(),
        "Client 0 should see entity_for_1 as Interpolated"
    );

    // Verify on client 1
    let e0_on_c1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_0)
        .copied()
        .unwrap();
    let e1_on_c1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity_for_1)
        .copied()
        .unwrap();

    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(e0_on_c1)
            .is_none(),
        "Client 1 should NOT see entity_for_0 as Predicted"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Interpolated>(e0_on_c1)
            .is_some(),
        "Client 1 should see entity_for_0 as Interpolated"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Predicted>(e1_on_c1)
            .is_some(),
        "Client 1 should see entity_for_1 as Predicted"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Interpolated>(e1_on_c1)
            .is_none(),
        "Client 1 should NOT see entity_for_1 as Interpolated"
    );
}

/// A child with explicitly set rooms keeps them (override):
/// plain children follow the root, but an overridden child replicates
/// independently and survives root room changes. Removing the override
/// follows the root's rooms again.
#[test]
fn a_child_with_its_own_rooms_overrides_the_root() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    let room_a = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_b = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_c = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();

    // client 0 is in room A, client 1 is in room B, nobody is in room C
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_0_entity)
        .insert(Rooms::single(room_a));
    let client_of_1_entity = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_1_entity)
        .insert(Rooms::single(room_b));

    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            CompFull(1.0),
            Rooms::single(room_a),
        ))
        .id();
    // plain child: rooms are followed on attach
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(server_parent), CompFull(2.0)))
        .id();
    // child spawned with different rooms: kept as an explicit override
    let server_other_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(server_parent), CompFull(3.0), Rooms::single(room_b)))
        .id();
    stepper.frame_step(2);

    assert_eq!(
        stepper.server_app().world().get::<Rooms>(server_child),
        Some(&Rooms::single(room_a)),
        "plain child should follow the root's rooms"
    );
    assert!(
        stepper
            .server_app()
            .world()
            .get::<Overridden<Rooms>>(server_child)
            .is_none(),
        "plain child should not be marked as overridden"
    );
    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<Rooms>(server_other_child),
        Some(&Rooms::single(room_b)),
        "explicit child rooms should be kept"
    );
    assert!(
        stepper
            .server_app()
            .world()
            .get::<Overridden<Rooms>>(server_other_child)
            .is_some(),
        "explicit child rooms should be marked as overridden"
    );

    let is_visible = |stepper: &Stepper<PredictionStack>, client: usize, entity: Entity| {
        stepper
            .client_app(client)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&entity)
            .copied()
            .is_some()
    };
    // parent and plain child follow room A; the override follows room B
    assert!(is_visible(&stepper, 0, server_parent));
    assert!(is_visible(&stepper, 0, server_child));
    assert!(!is_visible(&stepper, 0, server_other_child));
    assert!(!is_visible(&stepper, 1, server_parent));
    assert!(!is_visible(&stepper, 1, server_child));
    assert!(is_visible(&stepper, 1, server_other_child));

    // move the parent to room C: the plain child follows, the override survives
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_parent)
        .insert(Rooms::single(room_c));
    stepper.frame_step(2);

    assert_eq!(
        stepper.server_app().world().get::<Rooms>(server_child),
        Some(&Rooms::single(room_c)),
        "plain child should follow the root's new rooms"
    );
    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<Rooms>(server_other_child),
        Some(&Rooms::single(room_b)),
        "override should survive the root's room change"
    );
    assert!(!is_visible(&stepper, 0, server_parent));
    assert!(!is_visible(&stepper, 0, server_child));
    assert!(!is_visible(&stepper, 1, server_parent));
    assert!(!is_visible(&stepper, 1, server_child));
    assert!(
        is_visible(&stepper, 1, server_other_child),
        "override child should stay visible in room B"
    );

    // removing the override follows the root's rooms again
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_other_child)
        .remove::<Overridden<Rooms>>();
    stepper.frame_step(2);

    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<Rooms>(server_other_child),
        Some(&Rooms::single(room_c)),
        "unmarked child should follow the root's rooms again"
    );
    assert!(
        !is_visible(&stepper, 1, server_other_child),
        "child should follow room C again"
    );
}

/// Switching the root's `PredictionTarget` reaches the hierarchy members:
/// the child follows the new target server-side and gains/loses `Predicted`
/// on the client.
#[test]
fn a_child_follows_a_root_prediction_target_switch() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let client_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;

    let server_root = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_id)),
        ))
        .id();
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn(ChildOf(server_root))
        .id();
    stepper.step(4, stepper.frame_interval(), StepOrder::ServerFirst);
    let client_child = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_child)
        .copied()
        .expect("child should be replicated");

    // the child cloned the root's target and the client predicts it
    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<PredictionTarget>(server_child),
        stepper
            .server_app()
            .world()
            .get::<PredictionTarget>(server_root),
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(client_child)
            .is_some(),
        "targeted client should predict the child"
    );

    // switch the root target away from the client, in place
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_root)
        .insert(PredictionTarget::to_clients(NetworkTarget::all_except([
            client_id,
        ])));
    stepper.step(6, stepper.frame_interval(), StepOrder::ServerFirst);

    // the child followed the switch server-side...
    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<PredictionTarget>(server_child),
        stepper
            .server_app()
            .world()
            .get::<PredictionTarget>(server_root),
    );
    // ...and the client stopped predicting it
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(client_child)
            .is_none(),
        "untargeted client should no longer predict the child"
    );
}

/// https://github.com/cBournhonesque/lightyear/issues/627
/// A predicted parent and its child keep their hierarchy on the client: the child's `ChildOf`
/// names the client's entity for the server's parent.
#[test]
fn a_predicted_hierarchy_keeps_its_child_of_on_the_client() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_child = stepper.server_app_mut().world_mut().spawn_empty().id();
    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .add_child(server_child)
        .id();
    stepper.frame_step(2);

    // check that the parent and child are spawned on the client
    let predicted_child = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_child)
        .copied()
        .expect("child entity was not replicated to client");
    let predicted_parent = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_parent)
        .copied()
        .expect("parent entity was not replicated to client");
    info!("parent: {predicted_parent:?}, child: {predicted_child:?}");

    // check that the parent-child hierarchy is maintained
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<ChildOf>(predicted_child)
            .expect("predicted child entity doesn't have a parent")
            .parent(),
        predicted_parent
    );
}
