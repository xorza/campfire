use crate::fixtures::prediction_stack::PredictionStack;
use crate::fixtures::steps::trigger_state_rollback;
use crate::prediction_counts::PredictionCounts;
use crate::prediction_despawn_commands_ext::PredictionDespawnCommandsExt;
use crate::prediction_despawned::PredictionDespawned;
use crate::prediction_history::PredictionHistory;
use bevy_app::FixedUpdate;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::Commands;
use bevy_ecs::system::EntityCommand;
use bevy_ecs::system::Res;
use bevy_replicon::prelude::Remote;
use bevy_replicon::prelude::Signature;
use campfire_net_connection::NetworkTarget;
use campfire_net_core::Ticked;
use campfire_net_core::{NetTick, PositiveTickSpan};
use campfire_net_despawns::DespawnReplicated;
use campfire_net_emulator::LinkModel;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryState;
use campfire_net_replication::PreSpawnedReceiver;
use campfire_net_replication::Predicted;
use campfire_net_replication::internals::ClientCopies;
use campfire_net_replication::internals::PROPAGATION_FRAMES;
use campfire_net_replication::internals::{CompMap, CompSimple};
use campfire_net_replication::{PreSpawned, PredictionTarget, Replicate};
use campfire_net_stepper::Init;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_sync::{InputTimelineSetup, SyncSetup};
use campfire_net_timeline::LocalTimeline;
use tracing::info;

#[test]
fn distinct_salts_give_distinct_hashes_on_one_tick() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    // check default compute hash, with multiple entities sharing the same tick
    // but using distinct salts to produce unique active pre-spawn hashes
    let [entity_1, entity_2] = stepper.client_fixed_update(0, |world| {
        [0, 1].map(|salt| {
            world
                .spawn((CompFull(1.0), PreSpawned::default_with_salt(salt)))
                .id()
        })
    });
    let spawn_tick = stepper.client_tick(0);
    let hash_1 = stepper
        .client_world(0)
        .get::<PreSpawned>(entity_1)
        .unwrap()
        .hash()
        .unwrap();
    let hash_2 = stepper
        .client_world(0)
        .get::<PreSpawned>(entity_2)
        .unwrap()
        .hash()
        .unwrap();
    assert_ne!(hash_1, hash_2);

    let receiver = stepper.client_world(0).resource::<PreSpawnedReceiver>();
    assert_eq!(receiver.unmatched().len(), 2);
    assert_eq!(receiver.unmatched().last(), Some(&(spawn_tick, entity_2)));

    // check that a PredictionHistory got added to the entity
    assert_eq!(
        stepper
            .client_world(0)
            .entity(entity_1)
            .get::<PredictionHistory<CompFull>>()
            .unwrap()
            .states()
            .next_back(),
        Some(Ticked::new(
            spawn_tick,
            &HistoryState::Updated(CompFull(1.0))
        ))
    );
}

/// Two unmatched pre-spawns of one hash are a caller's logic error, as only one of them could
/// match: the second fails its spawn.
#[test]
#[should_panic(expected = "which an unmatched pre-spawn holds already")]
fn a_duplicate_pre_spawn_hash_breaks_the_contract() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    stepper.client_fixed_update(0, |world| {
        [(); 2].map(|()| world.spawn(PreSpawned::new(1)).id())
    });
}

/// A sender-scoped pre-spawn mapping must only claim the matching local entity
/// on the selected client. Other clients still receive the replicated entity,
/// but Replicon should spawn it normally even if they have the same signature.
#[test]
fn a_link_scoped_signature_matches_only_on_that_link() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    let client_0_pre_spawn = stepper.client_fixed_update(0, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });
    let client_1_pre_spawn = stepper.client_fixed_update(1, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });

    let client_0_link = stepper.client_of_entity(0);
    let server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1).for_link(client_0_link),
            CompFull(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);

    let client_0_mapped = stepper.client_copy(0, server_pre_spawn).unwrap();
    let client_1_mapped = stepper.client_copy(1, server_pre_spawn).unwrap();

    assert_eq!(client_0_mapped, client_0_pre_spawn);
    assert_ne!(client_1_mapped, client_1_pre_spawn);
    assert!(
        stepper
            .server_world()
            .resource::<PreSpawnedReceiver>()
            .unmatched()
            .is_empty(),
        "authoritative sender pre-spawns should not enter the receiver timeout ledger"
    );
}

/// If rollback or local cleanup despawns an unmatched pre-spawned entity, a later
/// replayed entity with the same explicit hash must be able to use that hash
/// again. Otherwise the server entity would be unable to match the replacement
/// local entity and Replicon would spawn a duplicate predicted entity.
#[test]
fn a_pre_spawn_reuses_its_hash_after_an_unmatched_local_despawn() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let first_local = stepper.client_fixed_update(0, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });
    stepper.frame_step(1);

    DespawnReplicated.apply(stepper.client_world_mut(0).entity_mut(first_local));
    stepper.frame_step(1);

    let replacement_local = stepper.client_fixed_update(0, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });
    let server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1),
            CompFull(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.client_world(0).get_entity(first_local).is_err(),
        "the first unmatched local pre-spawn should stay despawned"
    );
    assert_eq!(
        stepper.client_copy(0, server_pre_spawn).unwrap(),
        replacement_local
    );
    assert!(
        stepper
            .client_world(0)
            .get::<Predicted>(replacement_local)
            .is_some(),
        "the replacement local pre-spawn should match the server entity"
    );
}

/// Client and server run the same system to pre-spawn an entity
/// Server's should take over authority over the entity
#[test]
fn a_matched_pre_spawn_hands_authority_to_the_server() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let client_pre_spawn =
        stepper.client_fixed_update(0, |world| world.spawn(PreSpawned::new(1)).id());
    let server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    // thanks to pre-spawning, a Confirmed entity has been spawned on the client
    // that Confirmed entity is replicate from server_pre_spawn
    // and has client_pre_spawn as predicted entity
    let _predicted = stepper
        .client_world(0)
        .get::<Predicted>(client_pre_spawn)
        .unwrap();

    assert_eq!(
        stepper.client_copy(0, server_pre_spawn).unwrap(),
        client_pre_spawn
    );
    assert!(
        stepper
            .client_world(0)
            .get::<Signature>(client_pre_spawn)
            .is_some(),
        "matched pre-spawn signatures should stay attached until despawn so Replicon can clear SignatureMap"
    );
}

#[derive(Debug, Resource)]
struct ReplayPreSpawnTick(NetTick);

fn replay_spawn_pre_spawn(
    tick: Res<'_, ReplayPreSpawnTick>,
    timeline: Res<'_, LocalTimeline>,
    mut commands: Commands<'_, '_>,
) {
    if timeline.tick() == tick.0 {
        commands.spawn((PreSpawned::new(1), CompFull(2.0), CompSimple(2.0)));
    }
}

/// A matched PreSpawned entity spawned after the rollback tick should be
/// despawned before replay. The replayed fixed systems can then spawn a fresh
/// matching entity instead of leaving a duplicate live entity behind.
#[test]
fn a_matched_pre_spawn_is_despawned_on_a_rollback_before_its_spawn_tick() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    stepper.frame_step(1);
    let client_pre_spawn = stepper.client_fixed_update(0, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });
    let spawn_tick = stepper.client_tick(0);
    let server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1),
            CompFull(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);

    assert_eq!(
        stepper.client_copy(0, server_pre_spawn).unwrap(),
        client_pre_spawn
    );
    assert!(
        stepper
            .client_world(0)
            .resource::<PreSpawnedReceiver>()
            .rollback()
            .iter()
            .any(|(tick, entity)| *tick == spawn_tick && *entity == client_pre_spawn),
        "matched pre-spawn should retain its spawn tick for rollback"
    );
    stepper
        .client_world_mut(0)
        .entity_mut(client_pre_spawn)
        .remove::<PreSpawned>();

    stepper
        .client_world_mut(0)
        .insert_resource(ReplayPreSpawnTick(spawn_tick));
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, replay_spawn_pre_spawn);

    trigger_state_rollback(&mut stepper, spawn_tick - 1);
    stepper.frame_step(1);

    assert!(
        stepper
            .client_world(0)
            .get_entity(client_pre_spawn)
            .is_err(),
        "matched PreSpawned entity should be despawned on rollback to before its spawn tick"
    );

    let replayed_entities = stepper
        .client_world_mut(0)
        .query_filtered::<Entity, (With<PreSpawned>, With<CompSimple>)>()
        .iter(stepper.client_world(0))
        .collect::<Vec<_>>();
    assert_eq!(
        replayed_entities.len(),
        1,
        "rollback replay should spawn exactly one replacement pre-spawn"
    );
    assert_ne!(
        replayed_entities[0], client_pre_spawn,
        "rollback replay should create a fresh pre-spawned entity"
    );
}

/// A matched PreSpawned entity that already existed at the rollback tick should
/// stay alive. The rollback should only restore its component state; replay must
/// not despawn it and expect a spawn system from an earlier tick to run again.
#[test]
fn a_matched_pre_spawn_is_kept_on_a_rollback_at_or_after_its_spawn_tick() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    stepper.frame_step(1);
    let client_pre_spawn = stepper.client_fixed_update(0, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });
    let spawn_tick = stepper.client_tick(0);
    let server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1),
            CompFull(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);

    assert_eq!(
        stepper.client_copy(0, server_pre_spawn).unwrap(),
        client_pre_spawn
    );

    stepper
        .client_world_mut(0)
        .insert_resource(ReplayPreSpawnTick(spawn_tick));
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, replay_spawn_pre_spawn);

    trigger_state_rollback(&mut stepper, spawn_tick);
    stepper.frame_step(1);

    assert!(
        stepper.client_world(0).get_entity(client_pre_spawn).is_ok(),
        "matched PreSpawned entity should stay alive when rollback starts at its spawn tick"
    );
    assert!(
        stepper
            .client_world(0)
            .get::<Predicted>(client_pre_spawn)
            .is_some(),
        "matched PreSpawned entity should remain predicted"
    );

    let replayed_entities = stepper
        .client_world_mut(0)
        .query_filtered::<Entity, (With<PreSpawned>, With<CompSimple>)>()
        .iter(stepper.client_world(0))
        .collect::<Vec<_>>();
    assert!(
        replayed_entities.is_empty(),
        "rollback from the spawn tick should not rerun the spawn tick or create a replacement"
    );
}

/// A matched pre-spawn should not overwrite the live predicted component with
/// the server init value. The server value is authoritative for its confirmed
/// tick, but the client may already have simulated ahead locally.
#[test]
fn a_pre_spawn_init_goes_to_history_and_keeps_the_live_value() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let client_pre_spawn = stepper.client_fixed_update(0, |world| {
        world.spawn((PreSpawned::new(1), CompFull(1.0))).id()
    });

    stepper.frame_step(1);

    stepper
        .client_world_mut(0)
        .get_mut::<CompFull>(client_pre_spawn)
        .unwrap()
        .0 = 2.0;

    let server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1),
            CompFull(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);

    assert_eq!(
        stepper.client_copy(0, server_pre_spawn).unwrap(),
        client_pre_spawn
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(client_pre_spawn)
            .unwrap(),
        &CompFull(2.0)
    );

    let confirmed_history = stepper
        .client_world(0)
        .get::<ConfirmedHistory<CompFull>>(client_pre_spawn)
        .unwrap();
    assert!(
        confirmed_history
            .into_iter()
            .any(|entry| entry.value == &CompFull(1.0)),
        "server init value should be recorded as confirmed history: {confirmed_history:?}"
    );
}

/// Client and server run the same system to pre-spawn an entity
/// The pre-spawn somehow fails on the client (no matching hash)
/// The server entity should just get normally Predicted on the client
///
/// If the Confirmed entity is despawned, the Predicted entity should be despawned
#[test]
fn an_unmatched_server_entity_is_predicted_normally() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    // spawn extra entities to check that EntityMapping works correctly with pre-spawning
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    let client_entity = stepper
        .client_world_mut(0)
        .query_filtered::<Entity, With<Remote>>()
        .single(stepper.client_world(0))
        .unwrap();

    // run pre-spawned entity on server.
    // for some reason the entity is not spawned on the client
    let server_entity_2 = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            ChildOf(server_entity),
            PreSpawned::new(7),
            CompMap(server_entity),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    // We couldn't match the entity based on hash
    // So we should have just spawned a predicted entity
    let client_entity_2 = stepper
        .client_copy(0, server_entity_2)
        .expect("entity was not replicated to client");

    // the MapEntities component should have been mapped
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompMap>(client_entity_2)
            .unwrap()
            .0,
        client_entity
    );
}

/// Client spawns a PreSpawned entity and tries to despawn it locally
/// before it gets matched to a server entity.
/// The entity should be kept around in case of a match, and then cleanup via the cleanup system.
#[test]
fn a_local_despawn_before_a_match_keeps_the_pre_spawn_until_cleanup() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let client_pre_spawn = stepper.client_fixed_update(0, |world| {
        world
            .spawn((PreSpawned::new(1), CompFull(1.0), CompSimple(1.0)))
            .id()
    });
    stepper.frame_step(1);
    stepper
        .client_world_mut(0)
        .commands()
        .entity(client_pre_spawn)
        .prediction_despawn();
    stepper.frame_step(1);
    // check that the entity is disabled
    assert!(stepper.client_world(0).get_entity(client_pre_spawn).is_ok());
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(client_pre_spawn)
            .is_some()
    );

    // if enough frames pass without match, the entity gets cleaned
    stepper.frame_step(60);
    assert!(
        stepper
            .client_world(0)
            .get_entity(client_pre_spawn)
            .is_err()
    );
}

/// Client spawns a PreSpawned entity and tries to despawn it locally
/// before it gets matched to a server entity.
/// The match causes no rollback of its own, since the server components on the PreSpawned entity
/// match the client history when it was spawned. The server never despawns the entity, so the
/// predicted despawn is a misprediction: once a completed confirmed tick passes it, one rollback
/// brings the entity back.
#[test]
fn a_local_despawn_before_a_match_ends_on_the_match() {
    let mut setup = StepperSetup::single();
    setup.init = Init::Deferred;
    // Delay the server's packets so the client runs ahead of the server, and resyncs.
    setup.link.server_to_client = LinkModel::perfect().with_latency(2 * setup.tick_rate.period());
    let mut stepper = Stepper::new(PredictionStack, setup);
    let sync_setup = SyncSetup {
        max_error_margin: PositiveTickSpan::try_from(campfire_math::Num::HALF).unwrap(),
        ..SyncSetup::default()
    };
    stepper
        .client_world_mut(0)
        .insert_resource(InputTimelineSetup::default().with_sync_setup(sync_setup));
    stepper.init();

    let client_tick = stepper.client_tick(0).0 as usize;
    let server_tick = stepper.server_tick().0 as usize;
    info!(client_tick, server_tick);
    // The spawn's frame records the prediction history, so the predicted despawn after it keeps
    // the spawned values in the history.
    let client_pre_spawn = stepper.client_fixed_update(0, |world| {
        world
            .spawn((PreSpawned::new(1), CompFull(1.0), CompSimple(1.0)))
            .id()
    });
    stepper
        .client_world_mut(0)
        .commands()
        .entity(client_pre_spawn)
        .prediction_despawn();

    // we want to advance by the tick difference, so that the server pre-spawned is spawned on the
    // same tick as the client pre-spawned (i.e. entity is spawned on tick client_tick = X on
    // client, and spawned on tick server_tick = X on server, so that the Histories match)
    stepper.frame_step(client_tick - (server_tick + 1));
    let server_tick = stepper.server_tick().0 as usize;
    info!(server_tick);

    // make sure that the client_pre_spawn entity was disabled
    assert!(stepper.client_world(0).get_entity(client_pre_spawn).is_ok());
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(client_pre_spawn)
            .is_some()
    );

    // spawn the server pre-spawned entity
    let _server_pre_spawn = stepper
        .server_world_mut()
        .spawn((
            PreSpawned::new(1),
            CompFull(1.0),
            CompSimple(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    // advance enough so that the server entity is received
    stepper.frame_step(5);

    // the server entity gets replicated to the client
    // we should have a match with no rollbacks since the history matches with the confirmed state
    let _confirmed = stepper
        .client_world(0)
        .get::<Predicted>(client_pre_spawn)
        .unwrap();
    stepper.frame_step(10);
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<PredictionCounts>()
            .rollbacks,
        1
    );
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(client_pre_spawn)
            .is_none()
    );
}
