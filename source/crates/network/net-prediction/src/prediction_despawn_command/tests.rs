use bevy_replicon::prelude::Remote;
use campfire_net_connection::{NetworkTarget, NetworkTopology};
use campfire_net_core::NetTick;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::internals::ClientCopies;
use campfire_net_replication::internals::CompSimple;
use campfire_net_replication::internals::PROPAGATION_FRAMES;
use campfire_net_replication::{PredictionTarget, Replicate};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

use super::*;
use crate::fixtures::prediction_stack::PredictionStack;
use crate::prediction_despawn_commands_ext::PredictionDespawnCommandsExt;

#[derive(Component, Debug, PartialEq)]
struct TestComponent(usize);

/// If a predicted entity gets despawned erroneously
/// The rollback re-adds the predicted entity.
#[test]
fn rollback_restores_a_prediction_despawned_entity() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            CompFull(1.0),
            CompSimple(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    let predicted_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity is not present in entity map");

    // check that a rollback occurred to add the components on the predicted entity
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_entity)
            .unwrap(),
        &CompFull(1.0)
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompSimple>(predicted_entity)
            .unwrap(),
        &CompSimple(1.0)
    );
    // try adding a non-protocol component (which could be some rendering component)
    stepper
        .client_world_mut(0)
        .entity_mut(predicted_entity)
        .insert(TestComponent(1));

    // despawn the predicted entity locally
    stepper
        .client_world_mut(0)
        .commands()
        .entity(predicted_entity)
        .prediction_despawn();
    stepper.frame_step(1);
    // make sure that the entity is disabled
    assert!(stepper.client_world(0).get_entity(predicted_entity).is_ok());
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(predicted_entity)
            .is_some()
    );

    assert!(
        stepper
            .client_world(0)
            .get::<Remote>(predicted_entity)
            .is_some()
    );
    assert!(
        stepper
            .client_world(0)
            .get::<Predicted>(predicted_entity)
            .is_some()
    );

    // update the server entity to trigger a rollback where the predicted entity should be
    // 're-spawned'
    stepper
        .server_world_mut()
        .get_mut::<CompFull>(server_entity)
        .unwrap()
        .0 = 2.0;
    // Step until the client receives the mismatch and rollback re-enables the
    // predicted entity. Rounded timeline resyncs can shift this by a frame.
    let mut rollback_completed = false;
    for _ in 0..8 {
        stepper.frame_step(1);
        let world = stepper.client_world(0);
        let despawned_removed = world.get::<PredictionDespawned>(predicted_entity).is_none();
        let state_updated = world
            .get::<CompFull>(predicted_entity)
            .is_some_and(|value| value == &CompFull(2.0));
        if despawned_removed && state_updated {
            rollback_completed = true;
            break;
        }
    }
    assert!(
        rollback_completed,
        "predicted entity should be re-enabled and updated after rollback"
    );

    // Check that the entity was rolled back and the PredictionDespawned marker was removed
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(predicted_entity)
            .is_none()
    );

    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_entity)
            .unwrap(),
        &CompFull(2.0)
    );
    // non-Full components are also present
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompSimple>(predicted_entity)
            .unwrap(),
        &CompSimple(1.0)
    );
}

/// A world that predicts: a prediction setup, and a client topology.
fn predicting_world() -> World {
    let mut world = World::new();
    world.insert_resource(PredictionSetup::default());
    world.insert_resource(LocalTimeline::default());
    let client = world.spawn_empty().id();
    let mut networking = NetworkingState::default();
    networking.topology = NetworkTopology::Client(client);
    world.insert_resource(networking);
    world
}

/// Without a client topology the app does not predict, even with a prediction setup, and a
/// prediction despawn despawns.
#[test]
fn a_prediction_despawn_despawns_where_nothing_predicts() {
    let mut world = World::new();
    world.insert_resource(PredictionSetup::default());
    world.insert_resource(LocalTimeline::default());
    world.init_resource::<NetworkingState>();
    let entity = world.spawn(Predicted).id();

    PredictionDespawnCommand { entity }.apply(&mut world);

    assert!(world.get_entity(entity).is_err());
}

/// While the app predicts, a prediction despawn disables a predicted entity at the current tick.
#[test]
fn a_prediction_despawn_disables_where_the_app_predicts() {
    let mut world = predicting_world();
    world.resource_mut::<LocalTimeline>().shift(7);
    let entity = world.spawn(Predicted).id();

    PredictionDespawnCommand { entity }.apply(&mut world);

    assert_eq!(
        world.get::<PredictionDespawned>(entity),
        Some(&PredictionDespawned { tick: NetTick(7) })
    );
    // Outside `FixedMain`, which no replay runs, an entity this end alone predicts goes at once,
    // as no rollback must bring it back.
    let local = world.spawn(LocallyPredicted::default()).id();
    PredictionDespawnCommand { entity: local }.apply(&mut world);
    assert!(world.get_entity(local).is_err());
}

#[test]
#[should_panic(expected = "prediction_despawn is only for predicted entities")]
fn a_prediction_despawn_of_an_unpredicted_entity_panics() {
    let mut world = predicting_world();
    let entity = world.spawn_empty().id();

    PredictionDespawnCommand { entity }.apply(&mut world);
}
