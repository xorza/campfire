use super::*;
use crate::fixtures::prediction_stack::PredictionStack;
use crate::prediction_registration_ext::PredictionRegistrationExt;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_time::Virtual;
use campfire_math::TickRate;
use campfire_net_connection::NetworkTarget;
use campfire_net_core::Ticked;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::AppComponentExt;
use campfire_net_replication::ComponentRegistry;
use campfire_net_replication::internals::ClientCopies;
use campfire_net_replication::internals::PROPAGATION_FRAMES;

use bevy_ecs::system::RunSystemOnce;

use core::fmt::Debug;

use crate::fixtures::protocol::{CompNotNetworked, CompPredictionOnly};
use crate::fixtures::steps::{request_forced_rollback_at_completed_tick, trigger_state_rollback};
use crate::prediction_despawn_commands_ext::PredictionDespawnCommandsExt;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::prelude::RepliconTick;
use bevy_replicon::shared::replication::diff::diff_index::DiffIndex;
use campfire_net_replication::internals::CompRepliconDiff;
use campfire_net_replication::{HistoryDiffReceiver, PredictionTarget, Replicate};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_timeline::LocalTimelineShift;
use core::time::Duration;

mod pre_spawned;

#[derive(Component, Clone, PartialEq, Debug)]
struct TestComponent(f32);

fn register_test_rollback(world: &mut World) {
    world.init_resource::<ComponentRegistry>();
    world.init_resource::<PredictionRegistry>();
    world.register_component::<TestComponent>();
    let prediction_history_id = world.register_component::<PredictionHistory<TestComponent>>();
    world
        .resource_mut::<PredictionRegistry>()
        .register_rollback::<TestComponent>(prediction_history_id);
}

/// Rollback does not remove a predicted component
/// when the rollback tick predates the first retained history entry.
/// The history is then seeded with the current component value at the
/// rollback tick, anchoring the replay/checksum base.
#[test]
fn rollback_before_the_history_keeps_the_predicted_component() {
    let rollback_tick = NetTick(10);
    let mut world = World::new();
    world.init_resource::<LocalTimeline>();
    world.init_resource::<PredictionRegistry>();
    register_test_rollback(&mut world);

    world.insert_resource(PredictionSetup::default());
    world.insert_resource(Rollback(rollback_tick));

    let mut history = PredictionHistory::<TestComponent>::default();
    history.add(rollback_tick + 5, Some(TestComponent(1.0)));
    let predicted = world.spawn((Predicted, TestComponent(1.0), history)).id();

    world
        .run_system_once(RollbackPlugin::prepare_rollback::<TestComponent>)
        .unwrap();

    assert_eq!(
        world.get::<TestComponent>(predicted),
        Some(&TestComponent(1.0))
    );
    // No history sample existed at-or-before the rollback tick, so the
    // history is seeded with the current component value.
    assert_eq!(
        world
            .get::<PredictionHistory<TestComponent>>(predicted)
            .unwrap()
            .get_state_at_or_before(rollback_tick),
        Some(&HistoryState::Updated(TestComponent(1.0)))
    );
}

/// When the rollback tick predates the entity's confirmed history, the
/// history seed is the oldest present confirmed sample — the nearest
/// known data to the target — rather than the current live value. Live
/// itself is still left in place.
#[test]
fn a_rollback_before_the_history_starts_from_its_oldest_confirmed_value() {
    let rollback_tick = NetTick(10);
    let mut world = World::new();
    world.init_resource::<LocalTimeline>();
    world.init_resource::<PredictionRegistry>();
    register_test_rollback(&mut world);

    world.insert_resource(PredictionSetup::default());
    world.insert_resource(Rollback(rollback_tick));

    let mut history = PredictionHistory::<TestComponent>::default();
    history.add(NetTick(15), Some(TestComponent(99.0)));
    let mut confirmed = ConfirmedHistory::<TestComponent>::default();
    confirmed.insert_present(NetTick(12), TestComponent(12.0));
    confirmed.insert_present(NetTick(14), TestComponent(14.0));
    let entity = world
        .spawn((Predicted, TestComponent(99.0), history, confirmed))
        .id();

    world
        .run_system_once(RollbackPlugin::prepare_rollback::<TestComponent>)
        .unwrap();

    assert_eq!(
        world.get::<TestComponent>(entity),
        Some(&TestComponent(99.0))
    );
    assert_eq!(
        world
            .get::<PredictionHistory<TestComponent>>(entity)
            .unwrap()
            .get_state_at_or_before(rollback_tick),
        Some(&HistoryState::Updated(TestComponent(12.0)))
    );
}

/// Rollback churn (two rollbacks in one window, the second deeper than
/// the first): the first rollback must discard only the entries it
/// invalidates, so the deeper rollback still finds an exact per-tick
/// floor sample instead of falling through to seeding the history with
/// the current-tick live value — which would inject a future value into
/// the past and permanently diverge the replay.
#[test]
fn a_rollback_keeps_the_history_before_its_tick_for_a_deeper_one() {
    let mut world = World::new();
    world.init_resource::<LocalTimeline>();
    world.init_resource::<PredictionRegistry>();
    register_test_rollback(&mut world);

    world.insert_resource(PredictionSetup::default());

    let mut history = PredictionHistory::<TestComponent>::default();
    for tick in [8, 10, 12, 15] {
        history.add(NetTick(tick), Some(TestComponent(tick as f32)));
    }
    let predicted = world.spawn((Predicted, TestComponent(15.0), history)).id();

    // First rollback to tick 12: entries at-or-before 12 are preserved,
    // strictly newer entries are discarded, and the component restores
    // to the floor sample at the target.
    world.insert_resource(Rollback(NetTick(12)));
    world
        .run_system_once(RollbackPlugin::prepare_rollback::<TestComponent>)
        .unwrap();

    let history = world
        .get::<PredictionHistory<TestComponent>>(predicted)
        .unwrap();
    assert_eq!(
        history.get_state_at_or_before(NetTick(8)),
        Some(&HistoryState::Updated(TestComponent(8.0))),
        "an entry before the rollback target must be preserved"
    );
    assert_eq!(
        history.get_state_at_or_before(NetTick(15)),
        Some(&HistoryState::Updated(TestComponent(12.0))),
        "entries newer than the rollback target are discarded (the floor \
         sample at a later tick is the restored target value)"
    );
    assert_eq!(
        world.get::<TestComponent>(predicted),
        Some(&TestComponent(12.0)),
        "the component restores to the floor sample at the rollback target"
    );

    // Second, DEEPER rollback to tick 8: restores from the preserved
    // per-tick sample, not from the live post-first-rollback value.
    world.insert_resource(Rollback(NetTick(8)));
    world
        .run_system_once(RollbackPlugin::prepare_rollback::<TestComponent>)
        .unwrap();
    assert_eq!(
        world.get::<TestComponent>(predicted),
        Some(&TestComponent(8.0)),
        "the deeper rollback restores the preserved per-tick sample"
    );
}

/// https://github.com/cBournhonesque/lightyear/issues/1692
/// A Full-mode component the server inserts MID-GAME on an already-predicted
/// entity — one the client never predicts itself (think a server-authoritative
/// `Dead` marker) — must still reach the Predicted entity, even when the
/// client's prediction never mispredicts.
///
/// Without the fix, the confirmed insert is parked in `ConfirmedHistory<C>`:
/// the completed-confirmed tick scan returns early without a retained predicted
/// sample, and `RollbackPlugin::prepare_rollback` skips the component entirely because the
/// entity has no `PredictionHistory<C>` — so the component only ever arrives
/// as a side effect of a rollback caused by something else.
#[test]
fn a_late_server_insert_of_an_unpredicted_component_reaches_the_predicted_entity() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    // Spawn a predicted entity WITHOUT CompFull.
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let predicted = stepper
        .client_copy(0, server_entity)
        .expect("entity was not replicated to client");
    assert!(stepper.client_world(0).get::<CompFull>(predicted).is_none());

    // Settle well past the spawn so this is a true mid-game insert, not part
    // of the initial sync.
    stepper.frame_step(10);

    // The server inserts the component mid-game. The client has no system
    // predicting CompFull and nothing else mispredicts, so no rollback fires
    // for any other reason.
    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .insert(CompFull(42.0));

    // Replication + the completed-mutate-tick scan get time to deliver.
    stepper.frame_step(20);

    assert_eq!(
        stepper.client_world(0).get::<CompFull>(predicted),
        Some(&CompFull(42.0)),
        "server-inserted Full-mode component never reached the Predicted entity (#1692)"
    );
}

fn setup() -> (Stepper<PredictionStack>, Entity) {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    // add predicted/confirmed entities
    let predicted = stepper
        .client_world_mut(0)
        .spawn((Predicted, CompFull(1.0)))
        .id();
    // run one frame to initialize prediction history for the entity
    stepper.frame_step(1);
    (stepper, predicted)
}

fn insert_confirmed<C: Component + PartialEq>(
    world: &mut World,
    entity: Entity,
    tick: NetTick,
    value: Option<C>,
) {
    let state = match value {
        Some(value) => HistoryState::Updated(value),
        None => HistoryState::Removed,
    };
    let mut entity_mut = world.entity_mut(entity);
    if let Some(mut history) = entity_mut.get_mut::<ConfirmedHistory<C>>() {
        history.insert(tick, state);
    } else {
        let mut history = ConfirmedHistory::<C>::default();
        history.insert(tick, state);
        entity_mut.insert(history);
    }
}

fn record_completed_mutate_tick(world: &mut World, replicon_tick: RepliconTick, tick: NetTick) {
    // The server may have completed the tick already: a second confirmation of it is more
    // messages than the tick had.
    let mut mutate_ticks = world.resource_mut::<ServerMutateTicks>();
    if !mutate_ticks.contains(replicon_tick) {
        mutate_ticks.confirm(replicon_tick, 1);
    }
    let mut confirmed_ticks = world.resource_mut::<ConfirmedTickMap>();
    confirmed_ticks.record(replicon_tick, tick);
    confirmed_ticks.record_last_confirmed_tick(replicon_tick);
}

/// The newest tick whose mutations the client received. A fake completion at or before it meets
/// no server message still on its way, which would complete the tick a second time.
fn last_received_tick(stepper: &Stepper<PredictionStack>) -> NetTick {
    NetTick(
        stepper
            .client_world(0)
            .resource::<ServerMutateTicks>()
            .last_tick()
            .get(),
    )
}

/// The ticks a rollback to `rollback_tick` and the next frame's tick run, from `current_tick`:
/// the replay of each tick after the rollback tick, then the next tick.
fn replayed_ticks(current_tick: NetTick, rollback_tick: NetTick) -> f32 {
    f32::from(i16::try_from(current_tick + 1 - rollback_tick).unwrap())
}

fn reset_confirmed_tick_state(world: &mut World) {
    world.insert_resource(ServerMutateTicks::default());
    world.insert_resource(ConfirmedTickMap::default());
    world.insert_resource(RollbackCheckState::default());
}

#[derive(Debug, Resource, Default)]
struct ObservedRollbackStart(Option<NetTick>);

fn record_rollback_start(
    rollback: Option<Res<'_, Rollback>>,
    mut observed: ResMut<'_, ObservedRollbackStart>,
) {
    if observed.0.is_none() {
        observed.0 = rollback.map(|rollback| rollback.0);
    }
}

fn observe_rollback_start(app: &mut App) {
    app.insert_resource(ObservedRollbackStart::default());
    app.add_systems(
        PreUpdate,
        record_rollback_start
            .after(RollbackSystems::Check)
            .before(RollbackSystems::Prepare),
    );
}

fn mark_received_state_for_test(mut metadata: ResMut<'_, RollbackCheckState>) {
    metadata.received_messages_this_frame = true;
}

/// Client prediction adds a non-networked component.
/// On state rollback, missing confirmed state should not be treated as
/// authoritative removal.
#[test]
fn a_rollback_keeps_an_insert_that_has_no_confirmed_history() {
    let (mut stepper, predicted) = setup();

    stepper.frame_step(1);
    let rollback_tick = stepper.client_tick(0);
    stepper.frame_step(1);

    // Client prediction adds CompNotNetworked (not replicated, server doesn't have it)
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompNotNetworked(1.0));

    // Simulate confirmed state at rollback_tick: CompFull still 1.0 (no change from server)
    insert_confirmed(
        stepper.client_world_mut(0),
        predicted,
        rollback_tick,
        Some(CompFull(1.0)),
    );

    request_forced_rollback_at_completed_tick(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    // CompNotNetworked should remain: there was no authoritative removal.
    assert!(
        stepper
            .client_world(0)
            .get::<CompNotNetworked>(predicted)
            .is_some(),
        "State rollback should not remove a component only because it has no confirmed history"
    );
}

fn increment_and_remove(
    mut commands: Commands<'_, '_>,
    mut query: Query<'_, '_, (Entity, &mut CompFull), With<Predicted>>,
) {
    for (entity, mut comp) in query.iter_mut() {
        comp.0 += 1.0;
        if comp.0 == 5.0 {
            commands.entity(entity).remove::<CompFull>();
        }
    }
}

/// Client prediction removes a component, but server still has it.
/// On rollback, the component should be restored.
#[test]
fn a_rollback_restores_a_predicted_removal() {
    let (mut stepper, predicted) = setup();

    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_and_remove);

    // Run until CompFull is removed (1.0 → 2.0 → 3.0 → 4.0 → 5.0 → removed)
    stepper.frame_step(5);
    assert!(
        stepper.client_world(0).get::<CompFull>(predicted).is_none(),
        "CompFull should have been removed by prediction"
    );

    let tick = stepper.client_tick(0);
    let rollback_tick = last_received_tick(&stepper);
    // The server confirms CompFull = -10.0 at the rollback tick: it still exists.
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompFull(-10.0));
    insert_confirmed(
        stepper.client_world_mut(0),
        predicted,
        rollback_tick,
        Some(CompFull(-10.0)),
    );

    request_forced_rollback_at_completed_tick(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    // CompFull is re-added and re-simulated: -10 plus one increment for each tick run.
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted)
            .unwrap()
            .0,
        -10.0 + replayed_ticks(tick, rollback_tick),
        "Predicted-removed component should be restored and re-simulated on rollback"
    );
}

/// Client uses `prediction_despawn` on an entity.
/// On rollback (to before the despawn), the entity should be re-enabled.
#[test]
fn a_rollback_restores_a_predicted_despawn() {
    let (mut stepper, predicted) = setup();

    stepper.frame_step(1);
    let rollback_tick = stepper.client_tick(0);
    stepper.frame_step(1);

    // Predicted-despawn the entity (adds PredictionDespawned, doesn't actually despawn)
    stepper
        .client_world_mut(0)
        .commands()
        .entity(predicted)
        .prediction_despawn();
    stepper.frame_step(1);

    assert!(
        stepper.client_world(0).get_entity(predicted).is_ok(),
        "Entity should still exist after prediction_despawn"
    );
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(predicted)
            .is_some(),
        "Entity should have PredictionDespawned marker"
    );

    // Trigger rollback to before the despawn
    request_forced_rollback_at_completed_tick(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    // PredictionDespawned should be removed, entity re-enabled
    assert!(
        stepper.client_world(0).get_entity(predicted).is_ok(),
        "Entity should still exist after rollback"
    );
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionDespawned>(predicted)
            .is_none(),
        "PredictionDespawned should be removed after rollback"
    );
    assert_eq!(
        stepper.client_world(0).get::<CompFull>(predicted).unwrap(),
        &CompFull(1.0),
        "Component should be restored to value at rollback tick"
    );
}

/// A LocallyPredicted entity spawned in the simulation of a tick is despawned by a rollback to
/// before that tick, as the replay spawns it again; one spawned outside `FixedMain` stays, as no
/// replay spawns it again.
#[test]
fn a_rollback_despawns_a_predicted_spawn() {
    let (mut stepper, _) = setup();
    stepper.frame_step(1);

    let predicted_a = stepper.client_fixed_update(0, |world| {
        world
            .spawn((Predicted, LocallyPredicted::default(), CompFull(1.0)))
            .id()
    });
    let tick = stepper.client_tick(0);
    let outside = stepper
        .client_world_mut(0)
        .spawn((Predicted, LocallyPredicted::default(), CompFull(1.0)))
        .id();

    // trigger a rollback to before the entity was spawned
    trigger_state_rollback(&mut stepper, tick - 1);
    stepper.frame_step(1);

    assert!(stepper.client_world(0).get_entity(outside).is_ok());

    assert!(
        stepper.client_world(0).get_entity(predicted_a).is_err(),
        "Predicted-spawned entity should be despawned on rollback to before spawn tick"
    );
}

fn increment_component(mut query: Query<'_, '_, &mut CompFull, With<Predicted>>) {
    for mut comp in query.iter_mut() {
        comp.0 += 1.0;
    }
}

/// Client prediction modifies a component value differently than the server.
/// On rollback, the component should snap to the confirmed value and re-simulate.
#[test]
fn a_rollback_corrects_a_predicted_change() {
    let (mut stepper, predicted) = setup();

    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_component);

    // Run 3 frames: CompFull goes 1.0 → 2.0 → 3.0 → 4.0
    stepper.frame_step(3);
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted)
            .unwrap()
            .0,
        4.0
    );

    let tick = stepper.client_tick(0);
    // Server says CompFull was actually 10.0 at tick-2 (different from predicted 2.0)
    insert_confirmed(
        stepper.client_world_mut(0),
        predicted,
        tick - 2,
        Some(CompFull(10.0)),
    );

    request_forced_rollback_at_completed_tick(&mut stepper, tick - 2);
    stepper.frame_step(1);

    // Snap to 10.0 at tick-2, re-simulate 3 ticks: 10.0 + 3 = 13.0
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted)
            .unwrap()
            .0,
        13.0,
        "Modified component should be corrected to confirmed value and re-simulated"
    );
}

/// If one predicted entity triggers a rollback from an older tick while another
/// predicted entity already has a newer confirmed tick, the newer confirmed
/// value must be preserved during replay.
#[test]
fn a_rollback_keeps_the_later_confirmed_values_of_other_entities() {
    let (mut stepper, predicted_a) = setup();
    let predicted_b = stepper
        .client_world_mut(0)
        .spawn((Predicted, CompFull(10.0)))
        .id();

    // Initialize prediction history for the second entity.
    stepper.frame_step(1);
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_component);

    // Build enough history so rollback and later confirmed ticks are distinct.
    stepper.frame_step(4);
    let current_tick = stepper.client_tick(0);
    let rollback_tick = current_tick - 3;
    let later_confirmed_tick = current_tick - 1;

    let rollback_replicon_tick = RepliconTick::new(rollback_tick.0);
    let later_replicon_tick = RepliconTick::new(later_confirmed_tick.0);

    let world = stepper.client_world_mut(0);
    world
        .resource_mut::<ConfirmedTickMap>()
        .record(RepliconTick::default(), rollback_tick);
    world
        .resource_mut::<ConfirmedTickMap>()
        .record(rollback_replicon_tick, rollback_tick);
    world
        .resource_mut::<ConfirmedTickMap>()
        .record(later_replicon_tick, later_confirmed_tick);
    insert_confirmed(world, predicted_a, rollback_tick, Some(CompFull(100.0)));
    world
        .entity_mut(predicted_a)
        .insert(ConfirmHistory::new(rollback_replicon_tick));

    insert_confirmed(
        world,
        predicted_b,
        later_confirmed_tick,
        Some(CompFull(200.0)),
    );
    world
        .entity_mut(predicted_b)
        .insert(ConfirmHistory::new(later_replicon_tick));

    trigger_state_rollback(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_a)
            .unwrap()
            .0,
        104.0,
        "Rollback initiator should replay from the older confirmed tick through the current frame tick"
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_b)
            .unwrap()
            .0,
        // 200 is the state at the end of tick N - 1; ticks N and N + 1 add 1 each.
        202.0,
        "Later confirmed value on another entity should be preserved and replayed across the remaining rollback ticks and the current frame tick"
    );
}

/// Multiple explicit confirmed samples for the same component should survive an
/// older rollback. The rollback starts at the older mismatch, then replay still
/// snaps to the later confirmed sample.
#[test]
fn a_batch_of_confirmed_values_survives_an_older_rollback() {
    let (mut stepper, predicted) = setup();
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_component);

    stepper.frame_step(4);
    let current_tick = stepper.client_tick(0);
    let rollback_tick = last_received_tick(&stepper);
    let later_confirmed_tick = current_tick - 1;
    assert!(rollback_tick < later_confirmed_tick);

    let world = stepper.client_world_mut(0);
    insert_confirmed(world, predicted, rollback_tick, Some(CompFull(100.0)));
    insert_confirmed(
        world,
        predicted,
        later_confirmed_tick,
        Some(CompFull(200.0)),
    );

    request_forced_rollback_at_completed_tick(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    let confirmed_history = stepper
        .client_world(0)
        .get::<ConfirmedHistory<CompFull>>(predicted)
        .unwrap();
    assert!(
        confirmed_history
            .get_state_at(later_confirmed_tick)
            .is_some(),
        "later confirmed sample should survive rollback preparation"
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted)
            .unwrap()
            .0,
        // The confirmed 200 is a tick's end state; the two ticks after it add 1 each.
        202.0,
        "rollback replay should still snap to the later confirmed sample"
    );
}

#[test]
fn a_shift_of_the_local_timeline_leaves_the_confirmed_history() {
    let (mut stepper, predicted) = setup();

    let confirmed_tick = NetTick(10);
    insert_confirmed(
        stepper.client_world_mut(0),
        predicted,
        confirmed_tick,
        Some(CompFull(10.0)),
    );

    let predicted_tick = NetTick(5);
    let mut prediction_history = PredictionHistory::<CompFull>::default();
    prediction_history.add(predicted_tick, Some(CompFull(5.0)));
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(prediction_history);

    stepper
        .client_world_mut(0)
        .trigger(LocalTimelineShift { delta: 100 });

    let world = stepper.client_world(0);
    let confirmed_history = world
        .get::<ConfirmedHistory<CompFull>>(predicted)
        .expect("confirmed history should still be present");
    assert!(
        confirmed_history.get_state_at(confirmed_tick).is_some(),
        "confirmed history is already in authoritative server tick space and must not shift"
    );
    assert!(
        confirmed_history
            .get_state_at(confirmed_tick + 100)
            .is_none(),
        "timeline sync must not move authoritative confirmed samples into the future"
    );

    let prediction_history = world
        .get::<PredictionHistory<CompFull>>(predicted)
        .expect("prediction history should still be present");
    assert!(
        prediction_history
            .get_state_at_or_before(predicted_tick)
            .is_none(),
        "local prediction samples from before sync should be shifted"
    );
    assert!(
        prediction_history
            .get_state_at_or_before(predicted_tick + 100)
            .is_some(),
        "prediction history should follow the local timeline sync"
    );
}

/// A completed mutate tick is a global confirmation point. If one entity receives an
/// explicit update at that tick and another entity does not, the unchanged entity should
/// still be checked against its last confirmed value at the completed tick.
#[test]
fn a_completed_mutate_tick_checks_each_unchanged_entity() {
    let (mut stepper, updated) = setup();
    let unchanged = stepper
        .client_world_mut(0)
        .spawn((Predicted, CompFull(10.0)))
        .id();

    stepper.frame_step(1);
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_component);
    observe_rollback_start(stepper.client_app_mut(0));

    // Build a few ticks of prediction history after initializing the second entity.
    stepper.frame_step(4);
    let completed_tick = stepper.client_tick(0) - 2;
    let previous_confirmed_tick = completed_tick - 1;

    let updated_replicon_tick = RepliconTick::new(700);
    let previous_replicon_tick = RepliconTick::new(699);

    let world = stepper.client_world_mut(0);
    record_completed_mutate_tick(world, updated_replicon_tick, completed_tick);
    world
        .resource_mut::<ConfirmedTickMap>()
        .record(previous_replicon_tick, previous_confirmed_tick);

    insert_confirmed(world, updated, completed_tick, Some(CompFull(1.0)));
    world
        .entity_mut(updated)
        .insert(ConfirmHistory::new(updated_replicon_tick));

    insert_confirmed(
        world,
        unchanged,
        previous_confirmed_tick,
        Some(CompFull(20.0)),
    );
    world
        .entity_mut(unchanged)
        .insert(ConfirmHistory::new(previous_replicon_tick));

    stepper.frame_step(1);

    assert_eq!(
        stepper
            .client_world(0)
            .resource::<ObservedRollbackStart>()
            .0,
        Some(completed_tick),
        "Unchanged entity mismatch should roll back from the completed mutate tick"
    );
}

/// An entity-level confirmation at C can come from a different replicated component. It must not
/// suppress the check for a predicted component that was unchanged at C.
#[test]
fn a_completed_mutate_tick_checks_an_unchanged_component_of_a_confirmed_entity() {
    let (mut stepper, predicted) = setup();
    observe_rollback_start(stepper.client_app_mut(0));

    // This component will receive an explicit, matching authoritative update at C. Its presence
    // makes this a real mixed entity: one predicted component is updated while `CompFull` is
    // unchanged at C.
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompPredictionOnly(7.0));

    stepper.frame_step(4);
    let completed_tick = stepper.client_tick(0) - 2;
    let previous_confirmed_tick = completed_tick - 1;
    let completed_replicon_tick = RepliconTick::new(710);

    let world = stepper.client_world_mut(0);
    record_completed_mutate_tick(world, completed_replicon_tick, completed_tick);

    insert_confirmed(
        world,
        predicted,
        completed_tick,
        Some(CompPredictionOnly(7.0)),
    );
    // `CompFull` itself has no sample at C, so its effective authoritative value remains the
    // mismatching earlier value even though the same entity was explicitly updated at C.
    insert_confirmed(
        world,
        predicted,
        previous_confirmed_tick,
        Some(CompFull(20.0)),
    );
    assert_ne!(
        world
            .get::<PredictionHistory<CompFull>>(predicted)
            .and_then(|history| history.get(completed_tick)),
        Some(&CompFull(20.0)),
        "the unchanged CompFull must be mismatched in prediction history at C"
    );
    world
        .entity_mut(predicted)
        .insert(ConfirmHistory::new(completed_replicon_tick));

    stepper.frame_step(1);

    assert_eq!(
        stepper
            .client_world(0)
            .resource::<ObservedRollbackStart>()
            .0,
        Some(completed_tick),
        "An entity confirmation at C must not hide an unchanged component mismatch"
    );
}

/// A diff component does not need an explicit sample at every completed confirmed tick. Once all
/// mutate messages for C are received and no diff at or before C is pending, its last materialized
/// value is known to carry forward to C.
#[test]
fn the_confirmed_tick_advances_past_the_last_diff_update() {
    let (mut stepper, predicted) = setup();
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompRepliconDiff(1));
    stepper.frame_step(3);

    let completed_tick = stepper.client_tick(0) - 1;
    let last_diff_update_tick = completed_tick - 1;
    let completed_replicon_tick = RepliconTick::new(711);
    {
        let world = stepper.client_world_mut(0);
        reset_confirmed_tick_state(world);
        insert_confirmed(
            world,
            predicted,
            last_diff_update_tick,
            Some(CompRepliconDiff(1)),
        );
        assert_eq!(
            world
                .get::<ConfirmedHistory<CompRepliconDiff>>(predicted)
                .unwrap()
                .len(),
            1,
            "the setup should contain only the explicit diff update"
        );
        world
            .entity_mut(predicted)
            .insert(ConfirmHistory::new(completed_replicon_tick));
        record_completed_mutate_tick(world, completed_replicon_tick, completed_tick);
    }

    stepper.client_app_mut(0).update();

    let world = stepper.client_world(0);
    assert_eq!(
        world
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(completed_tick),
        "C must not be capped at the diff component's last explicit update"
    );
    assert_eq!(
        world
            .get::<ConfirmedHistory<CompRepliconDiff>>(predicted)
            .and_then(|history| history.get_state_at(completed_tick)),
        Some(&HistoryState::Updated(CompRepliconDiff(1))),
        "the confirmed tick should carry the last materialized diff value forward to C"
    );
    let history = world
        .get::<ConfirmedHistory<CompRepliconDiff>>(predicted)
        .unwrap();
    assert_eq!(history.len(), 2, "C should add one unchanged anchor");
    assert_eq!(
        history.get_nth_tick(1),
        Some(completed_tick),
        "the unchanged anchor should be stored exactly at C"
    );
}

/// With multiple predicted diff entities, C is the latest globally completed confirmed tick
/// strictly before the earliest unresolved P. Resolving pending ranges advances C through completed
/// ticks; ticks that were never completed must never become rollback targets.
#[test]
fn the_diff_confirmed_tick_follows_the_earliest_pending_diff_of_all_entities() {
    let (mut stepper, predicted_a) = setup();
    stepper
        .client_world_mut(0)
        .entity_mut(predicted_a)
        .insert(CompRepliconDiff(1));
    let predicted_b = stepper
        .client_world_mut(0)
        .spawn((Predicted, CompRepliconDiff(1)))
        .id();
    stepper.frame_step(6);

    let candidate_tick = stepper.client_tick(0) - 1;
    // A is pending exactly at C0. This exercises the strict boundary: C0 itself is unusable even
    // though ServerMutateTicks reports it as globally completed.
    let pending_a_tick = candidate_tick;
    let middle_completed_tick = candidate_tick - 2;
    let pending_b_tick = candidate_tick - 3;
    let oldest_completed_tick = candidate_tick - 4;
    let oldest_replicon_tick = RepliconTick::new(712);
    let middle_replicon_tick = RepliconTick::new(713);
    let candidate_replicon_tick = RepliconTick::new(714);
    {
        let world = stepper.client_world_mut(0);
        reset_confirmed_tick_state(world);
        for predicted in [predicted_a, predicted_b] {
            insert_confirmed(
                world,
                predicted,
                oldest_completed_tick,
                Some(CompRepliconDiff(1)),
            );
            world
                .entity_mut(predicted)
                .insert(ConfirmHistory::new(candidate_replicon_tick));
        }

        let mut receiver_a = HistoryDiffReceiver::<CompRepliconDiff>::default();
        receiver_a.record_cursor(oldest_completed_tick, Some(DiffIndex::new(0)));
        receiver_a
            .queue_diff(pending_a_tick, DiffIndex::new(2), vec![2])
            .unwrap();
        world
            .resource_mut::<ReplicationStorage>()
            .insert(predicted_a, receiver_a);

        let mut receiver_b = HistoryDiffReceiver::<CompRepliconDiff>::default();
        receiver_b.record_cursor(oldest_completed_tick, Some(DiffIndex::new(0)));
        receiver_b
            .queue_diff(pending_b_tick, DiffIndex::new(2), vec![2])
            .unwrap();
        world
            .resource_mut::<ReplicationStorage>()
            .insert(predicted_b, receiver_b);

        // B's pending tick is not globally completed. A's pending tick is the completed C0 itself.
        // In both cases, only these three explicitly completed ticks are eligible for C.
        record_completed_mutate_tick(world, oldest_replicon_tick, oldest_completed_tick);
        record_completed_mutate_tick(world, middle_replicon_tick, middle_completed_tick);
        record_completed_mutate_tick(world, candidate_replicon_tick, candidate_tick);
    }

    stepper.client_app_mut(0).update();
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(oldest_completed_tick),
        "B's earlier pending tick should bound C to the latest completed tick before it"
    );
    for predicted in [predicted_a, predicted_b] {
        assert!(
            stepper
                .client_world(0)
                .get::<ConfirmedHistory<CompRepliconDiff>>(predicted)
                .unwrap()
                .get_state_at(candidate_tick)
                .is_none(),
            "C0 must not receive an unchanged anchor while an earlier diff is pending"
        );
    }

    // Materializing B's pending range leaves A pending exactly at C0. C can advance to the
    // completed middle tick, but the strict bound must still exclude C0 itself.
    {
        let world = stepper.client_world_mut(0);
        insert_confirmed(
            world,
            predicted_b,
            pending_b_tick,
            Some(CompRepliconDiff(1)),
        );
        world
            .resource_mut::<ReplicationStorage>()
            .get_mut::<HistoryDiffReceiver<CompRepliconDiff>>(predicted_b)
            .unwrap()
            .record_cursor(pending_b_tick, Some(DiffIndex::new(2)));
    }
    stepper.client_app_mut(0).update();
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(middle_completed_tick)
    );

    // Once A also materializes, there is no pending diff at or before C0. The full confirmed tick
    // scan can use A's exact value at C0 and add a proven SameAsPrecedent anchor for unchanged B.
    {
        let world = stepper.client_world_mut(0);
        insert_confirmed(
            world,
            predicted_a,
            pending_a_tick,
            Some(CompRepliconDiff(1)),
        );
        world
            .resource_mut::<ReplicationStorage>()
            .get_mut::<HistoryDiffReceiver<CompRepliconDiff>>(predicted_a)
            .unwrap()
            .record_cursor(pending_a_tick, Some(DiffIndex::new(2)));
    }
    stepper.client_app_mut(0).update();
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(candidate_tick)
    );
    for predicted in [predicted_a, predicted_b] {
        assert_eq!(
            stepper
                .client_world(0)
                .get::<ConfirmedHistory<CompRepliconDiff>>(predicted)
                .and_then(|history| history.get_state_at(candidate_tick)),
            Some(&HistoryState::Updated(CompRepliconDiff(1))),
            "the final safe C should use the known authoritative value"
        );
    }

    // A pending diff newer than C0 does not affect the authoritative state at C0.
    {
        let world = stepper.client_world_mut(0);
        world
            .resource_mut::<ReplicationStorage>()
            .get_mut::<HistoryDiffReceiver<CompRepliconDiff>>(predicted_b)
            .unwrap()
            .queue_diff(candidate_tick + 1, DiffIndex::new(4), vec![4])
            .unwrap();
        world.insert_resource(RollbackCheckState::default());
    }
    stepper.client_app_mut(0).update();
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(candidate_tick),
        "a future pending diff must not constrain the current confirmed tick"
    );
}

/// If the earliest unresolved diff has no globally completed confirmed tick before it, there is no
/// coherent full-state rollback target. The client must wait without scanning C0 or advancing the
/// processed-confirmed tick watermark; a component-history entry alone cannot manufacture a C.
#[test]
fn a_pending_diff_with_no_earlier_confirmed_tick_skips_the_check() {
    let (mut stepper, predicted) = setup();
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompRepliconDiff(1));
    stepper.frame_step(3);

    let candidate_tick = stepper.client_tick(0) - 1;
    let pending_tick = NetTick(1);
    let history_tick = NetTick(0);
    let candidate_replicon_tick = RepliconTick::new(716);
    {
        let world = stepper.client_world_mut(0);
        reset_confirmed_tick_state(world);
        insert_confirmed(world, predicted, history_tick, Some(CompRepliconDiff(1)));
        world
            .entity_mut(predicted)
            .insert(ConfirmHistory::new(candidate_replicon_tick));

        let mut receiver = HistoryDiffReceiver::<CompRepliconDiff>::default();
        receiver.record_cursor(history_tick, Some(DiffIndex::new(0)));
        receiver
            .queue_diff(pending_tick, DiffIndex::new(2), vec![2])
            .unwrap();
        world
            .resource_mut::<ReplicationStorage>()
            .insert(predicted, receiver);

        // The component-history sample at tick 0 is not completed in ServerMutateTicks. All real
        // and synthetic completed confirmed ticks in this test are at or after P=1, so none can be
        // selected as the required strict predecessor.
        record_completed_mutate_tick(world, candidate_replicon_tick, candidate_tick);
    }
    observe_rollback_start(stepper.client_app_mut(0));

    stepper.client_app_mut(0).update();

    let world = stepper.client_world(0);
    assert_eq!(
        world
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        None,
        "without a completed C before P, rollback checking must wait"
    );
    assert_eq!(world.resource::<ObservedRollbackStart>().0, None);
    assert!(
        world
            .get::<ConfirmedHistory<CompRepliconDiff>>(predicted)
            .unwrap()
            .get_state_at(candidate_tick)
            .is_none(),
        "the unresolved C0 must not receive an unchanged anchor"
    );
}

/// `RollbackMode::Always` must use the same diff-aware C as mismatch checking. Rolling back from
/// the globally latest confirmed tick while an older diff is unresolved would restore a value that
/// has not been materialized yet.
#[test]
fn an_always_rollback_takes_the_confirmed_tick_the_diffs_allow() {
    let (mut stepper, predicted) = setup();
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompRepliconDiff(1));
    stepper.frame_step(3);

    let candidate_tick = stepper.client_tick(0) - 1;
    let pending_tick = candidate_tick - 1;
    let previous_tick = pending_tick - 1;
    let previous_replicon_tick = RepliconTick::new(714);
    let candidate_replicon_tick = RepliconTick::new(715);
    {
        let world = stepper.client_world_mut(0);
        reset_confirmed_tick_state(world);
        world.resource_mut::<PredictionSetup>().rollback_setup.mode = RollbackMode::Always;
        insert_confirmed(world, predicted, previous_tick, Some(CompRepliconDiff(1)));
        world
            .entity_mut(predicted)
            .insert(ConfirmHistory::new(candidate_replicon_tick));

        let mut receiver = HistoryDiffReceiver::<CompRepliconDiff>::default();
        receiver.record_cursor(previous_tick, Some(DiffIndex::new(0)));
        receiver
            .queue_diff(pending_tick, DiffIndex::new(2), vec![2])
            .unwrap();
        world
            .resource_mut::<ReplicationStorage>()
            .insert(predicted, receiver);

        record_completed_mutate_tick(world, previous_replicon_tick, previous_tick);
        record_completed_mutate_tick(world, candidate_replicon_tick, candidate_tick);
    }
    observe_rollback_start(stepper.client_app_mut(0));
    stepper.client_app_mut(0).add_systems(
        PreUpdate,
        mark_received_state_for_test
            .after(ClientSystems::Receive)
            .before(RollbackSystems::Check),
    );

    // Exercise the receive-time signal used by RollbackMode::Always deterministically; relying on
    // the connected test transport would make this assertion depend on whether that frame happened
    // to carry replication traffic.
    stepper.frame_step(1);

    let world = stepper.client_world(0);
    assert_eq!(
        world.resource::<ObservedRollbackStart>().0,
        Some(previous_tick),
        "Always mode should roll back from the latest confirmed tick before the pending diff"
    );
    assert_eq!(
        world
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(previous_tick)
    );
}

#[test]
fn no_confirmed_tick_at_or_before_local_tick_is_processed() {
    let (mut stepper, _) = setup();
    stepper.frame_step(3);
    stepper.client_app_mut(0).update();

    let local_tick = stepper.client_tick(0);
    let future_tick = stepper.client_tick(0) + 1_000;
    let future_replicon_tick = RepliconTick::new(920);
    {
        let world = stepper.client_world_mut(0);
        reset_confirmed_tick_state(world);
        record_completed_mutate_tick(world, future_replicon_tick, future_tick);
    }
    observe_rollback_start(stepper.client_app_mut(0));

    stepper.client_app_mut(0).update();

    assert_eq!(stepper.client_tick(0), local_tick);
    let world = stepper.client_world(0);
    assert_eq!(
        world
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        None,
        "there is no confirmed tick C at or before the local tick"
    );
    assert_eq!(world.resource::<ObservedRollbackStart>().0, None);
}

/// When the globally latest completed confirmed tick S is in the local future, rollback checking
/// must select the newest completed confirmed tick C at or before the local tick T.
#[test]
fn the_check_takes_the_latest_confirmed_tick_at_or_before_the_local_tick() {
    let (mut stepper, predicted) = setup();
    stepper.frame_step(3);
    stepper.client_app_mut(0).update();

    let local_tick = stepper.client_tick(0);
    let completed_tick = local_tick - 1;
    let future_tick = local_tick + 1;
    let completed_replicon_tick = RepliconTick::new(921);
    let future_replicon_tick = RepliconTick::new(922);
    {
        let world = stepper.client_world_mut(0);
        reset_confirmed_tick_state(world);
        insert_confirmed(world, predicted, completed_tick, Some(CompFull(42.0)));
        world
            .entity_mut(predicted)
            .insert(ConfirmHistory::new(completed_replicon_tick));
        record_completed_mutate_tick(world, completed_replicon_tick, completed_tick);
        record_completed_mutate_tick(world, future_replicon_tick, future_tick);
    }
    observe_rollback_start(stepper.client_app_mut(0));

    stepper.client_app_mut(0).update();

    assert_eq!(stepper.client_tick(0), local_tick);
    let world = stepper.client_world(0);
    assert_eq!(
        world.resource::<ConfirmedTickMap>().last_confirmed_tick(),
        Some(future_tick),
        "S should remain the globally latest confirmed tick"
    );
    assert_eq!(
        world
            .resource::<RollbackCheckState>()
            .last_processed_confirmed_tick(),
        Some(completed_tick),
        "C should be the latest confirmed tick at or before T"
    );
    assert_eq!(
        world.resource::<ObservedRollbackStart>().0,
        Some(completed_tick),
        "the mismatch should roll back from C, not future S"
    );
}

/// An update for the client's current tick is locally checkable because that fixed tick has
/// already completed and `FixedPostUpdate` has recorded its prediction history.
#[test]
fn a_confirmation_of_the_current_tick_starts_a_rollback() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            CompFull(1.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    // Replicate the predicted entity and run real fixed ticks on the client so its history is
    // populated through the normal FixedPostUpdate system.
    stepper.frame_step(PROPAGATION_FRAMES);
    let predicted_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity was not replicated to the client");
    let current_tick = stepper.client_tick(0);
    assert!(
        stepper
            .client_world(0)
            .get::<PredictionHistory<CompFull>>(predicted_entity)
            .and_then(|history| history.get_state_at_or_before(current_tick))
            .is_some(),
        "the current tick should have prediction history after its fixed schedule completed"
    );
    assert!(
        stepper.server_tick() < current_tick,
        "the synced client should be predicting ahead of the server"
    );

    observe_rollback_start(stepper.client_app_mut(0));
    stepper
        .client_world_mut(0)
        .resource_mut::<Time<Virtual>>()
        .pause();

    // Keep the client parked at its genuinely completed tick while the server catches up. On the
    // final server fixed tick, mutate the authoritative value and send it with exactly the client's
    // current tick. The client receives it in PreUpdate before running any further fixed ticks.
    while stepper.server_tick() + 1 < current_tick {
        stepper.advance_time(stepper.frame_interval());
        stepper.server_app_mut().update();
        // Receive each completed Replicon confirmed tick while the paused client remains at T.
        stepper.client_app_mut(0).update();
        assert_eq!(stepper.client_tick(0), current_tick);
    }
    assert_eq!(stepper.server_tick() + 1, current_tick);
    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .insert(CompFull(42.0));
    stepper.advance_time(stepper.frame_interval());
    stepper.server_app_mut().update();
    assert_eq!(stepper.server_tick(), current_tick);

    // Virtual time is paused, so this direct update receives the confirmation and performs the
    // rollback in PreUpdate without running another client fixed tick or changing current_tick.
    stepper.client_app_mut(0).update();
    assert_eq!(stepper.client_tick(0), current_tick);
    let world = stepper.client_world(0);
    assert_eq!(
        world.resource::<ConfirmedTickMap>().last_confirmed_tick(),
        Some(current_tick),
        "the server-complete frontier should reach the confirmation tick during Receive"
    );
    assert_eq!(
        world
            .get::<ConfirmedHistory<CompFull>>(predicted_entity)
            .and_then(ConfirmedHistory::newest_present),
        Some(Ticked::new(current_tick, &CompFull(42.0))),
        "the authoritative update should be received at the client's current tick"
    );
    assert!(
        world
            .get::<PredictionHistory<CompFull>>(predicted_entity)
            .and_then(|history| history.get_state_at_or_before(current_tick))
            .is_some(),
        "prediction history should already resolve the current tick when the update arrives"
    );
    assert_eq!(
        world.resource::<ObservedRollbackStart>().0,
        Some(current_tick),
        "a mismatch at the current completed tick should trigger rollback in the receiving update"
    );
    assert_eq!(
        world.get::<CompFull>(predicted_entity),
        Some(&CompFull(42.0)),
        "rollback should apply the current-tick authoritative value"
    );
}

/// A component inserted at a server tick ahead of the client seeds absence at the client's
/// current tick, then rolls back once that authoritative tick becomes locally checkable.
#[test]
fn a_future_diff_insert_starts_the_history_and_rolls_back_once_checkable() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);
    let predicted_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity was not replicated to the client");
    let current_tick = stepper.client_tick(0);
    let future_tick = current_tick + 3;

    observe_rollback_start(stepper.client_app_mut(0));
    stepper
        .client_world_mut(0)
        .resource_mut::<Time<Virtual>>()
        .pause();

    // Keep the client at its completed local tick while the server moves into its future. Let the
    // client receive each confirmed tick so Replicon's completed frontier advances normally.
    while stepper.server_tick() + 1 < future_tick {
        stepper.advance_time(stepper.frame_interval());
        stepper.server_app_mut().update();
        stepper.client_app_mut(0).update();
        assert_eq!(stepper.client_tick(0), current_tick);
    }
    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .insert(CompRepliconDiff(42));
    stepper.advance_time(stepper.frame_interval());
    stepper.server_app_mut().update();
    assert_eq!(stepper.server_tick(), future_tick);
    stepper.client_app_mut(0).update();

    let world = stepper.client_world(0);
    assert_eq!(
        world
            .get::<PredictionHistory<CompRepliconDiff>>(predicted_entity)
            .and_then(|history| history.get_state_at_or_before(current_tick)),
        Some(&HistoryState::Removed),
        "the future insert should seed absence at the client's current tick"
    );
    assert_eq!(
        world
            .get::<ConfirmedHistory<CompRepliconDiff>>(predicted_entity)
            .and_then(|history| history.get_state_at(future_tick)),
        Some(&HistoryState::Updated(CompRepliconDiff(42)))
    );
    assert_eq!(world.resource::<ObservedRollbackStart>().0, None);

    // Exercise the old ordering regression directly, then remove the probe so the remainder of
    // the test observes the real absent-component history produced by the client schedule.
    let later_prediction_tick = current_tick + 1;
    let mut history = stepper
        .client_world_mut(0)
        .get_mut::<PredictionHistory<CompRepliconDiff>>(predicted_entity)
        .unwrap();
    history.add(
        later_prediction_tick,
        Some(CompRepliconDiff(later_prediction_tick.0)),
    );
    assert_eq!(
        history.states().map(|entry| entry.tick).collect::<Vec<_>>(),
        [current_tick, later_prediction_tick],
        "later local predictions should remain chronologically ordered"
    );
    history.clear_after_tick(current_tick);
    assert_eq!(
        history.states().map(|entry| entry.tick).collect::<Vec<_>>(),
        [current_tick]
    );

    // Run genuine client fixed ticks until the future confirmation becomes the completed current
    // tick. The following no-time update performs the completed-confirmed tick scan before another
    // fixed tick.
    stepper
        .client_world_mut(0)
        .resource_mut::<Time<Virtual>>()
        .unpause();
    while stepper.client_tick(0) < future_tick {
        stepper.advance_time(stepper.frame_interval());
        stepper.client_app_mut(0).update();
    }

    let world = stepper.client_world(0);
    assert_eq!(world.resource::<ObservedRollbackStart>().0, None);
    let history = world
        .get::<PredictionHistory<CompRepliconDiff>>(predicted_entity)
        .expect("the predicted component should retain its absence history");
    assert_eq!(
        history.get_state_at_or_before(future_tick),
        Some(&HistoryState::Removed),
        "real client fixed ticks should record absence through the authoritative tick"
    );

    stepper
        .client_world_mut(0)
        .resource_mut::<Time<Virtual>>()
        .pause();
    stepper.client_app_mut(0).update();

    let world = stepper.client_world(0);
    assert_eq!(
        world.resource::<ObservedRollbackStart>().0,
        Some(future_tick)
    );
    assert_eq!(
        world.get::<CompRepliconDiff>(predicted_entity),
        Some(&CompRepliconDiff(42))
    );
}

/// When a completed mutate tick triggers rollback through an unchanged entity, later explicit
/// confirmed values on other entities must be preserved for replay.
#[test]
fn a_rollback_of_a_completed_mutate_tick_keeps_the_later_confirmed_values() {
    let (mut stepper, predicted_a) = setup();
    let predicted_b = stepper
        .client_world_mut(0)
        .spawn((Predicted, CompFull(10.0)))
        .id();
    let predicted_c = stepper
        .client_world_mut(0)
        .spawn((Predicted, CompFull(20.0)))
        .id();

    // Initialize prediction history for the second entity.
    stepper.frame_step(1);
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, increment_component);
    observe_rollback_start(stepper.client_app_mut(0));

    stepper.frame_step(4);
    let current_tick = stepper.client_tick(0);
    // The server completes the next tick during the next frame, which the rollback check then
    // takes as its newest completed confirmed tick.
    let rollback_tick = last_received_tick(&stepper) + 1;
    let previous_confirmed_tick = rollback_tick - 1;
    let later_confirmed_tick = current_tick - 1;
    assert!(rollback_tick < later_confirmed_tick);

    let previous_replicon_tick = RepliconTick::new(previous_confirmed_tick.0);
    let later_replicon_tick = RepliconTick::new(later_confirmed_tick.0);
    let same_replicon_tick = RepliconTick::new(rollback_tick.0);

    let world = stepper.client_world_mut(0);
    {
        let mut confirmed_ticks = world.resource_mut::<ConfirmedTickMap>();
        confirmed_ticks.record(same_replicon_tick, rollback_tick);
        confirmed_ticks.record(previous_replicon_tick, previous_confirmed_tick);
        confirmed_ticks.record(later_replicon_tick, later_confirmed_tick);
    }

    insert_confirmed(
        world,
        predicted_a,
        previous_confirmed_tick,
        Some(CompFull(100.0)),
    );
    world
        .entity_mut(predicted_a)
        .insert(ConfirmHistory::new(previous_replicon_tick));

    insert_confirmed(
        world,
        predicted_b,
        later_confirmed_tick,
        Some(CompFull(200.0)),
    );
    world
        .entity_mut(predicted_b)
        .insert(ConfirmHistory::new(later_replicon_tick));

    insert_confirmed(world, predicted_c, rollback_tick, Some(CompFull(300.0)));
    world
        .entity_mut(predicted_c)
        .insert(ConfirmHistory::new(same_replicon_tick));

    stepper.frame_step(1);

    assert_eq!(
        stepper
            .client_world(0)
            .resource::<ObservedRollbackStart>()
            .0,
        Some(rollback_tick),
        "Unchanged entity mismatch should roll back from the completed mutate tick"
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_a)
            .unwrap()
            .0,
        100.0 + replayed_ticks(current_tick, rollback_tick),
        "Unchanged entity should replay from the completed mutate tick"
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_b)
            .unwrap()
            .0,
        // The confirmed 200 is a tick's end state; the two ticks after it add 1 each.
        202.0,
        "Later confirmed value on another entity should be preserved across rollback"
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_c)
            .unwrap()
            .0,
        300.0 + replayed_ticks(current_tick, rollback_tick),
        "Entity already confirmed at the completed mutate tick should replay from that confirmed value"
    );
}

/// Replicated helper/input entities can carry `ConfirmHistory` without having any
/// prediction history. A stale confirm tick for those entities should not make
/// the unchanged-entity rollback pass resolve an evicted confirmed tick.
#[test]
fn a_stale_confirm_history_with_no_prediction_history_is_ignored() {
    let (mut stepper, _) = setup();
    stepper.frame_step(5);

    let server_confirmed_tick = stepper.client_tick(0) - 1;
    let current_replicon_tick = RepliconTick::new(500);
    let stale_replicon_tick = RepliconTick::new(1);

    let world = stepper.client_world_mut(0);
    world.spawn((Predicted, ConfirmHistory::new(stale_replicon_tick)));
    record_completed_mutate_tick(world, current_replicon_tick, server_confirmed_tick);

    stepper.frame_step(1);
}

#[derive(Debug, Resource, Default)]
struct RollbackObserved(bool);

fn record_rollback(
    rollback: Option<Res<'_, Rollback>>,
    mut observed: ResMut<'_, RollbackObserved>,
) {
    observed.0 |= rollback.is_some();
}

#[test]
fn a_confirm_history_tick_with_no_mapping_asks_for_no_rollback() {
    let (mut stepper, predicted) = setup();
    stepper.frame_step(5);

    let server_confirmed_tick = stepper.client_tick(0) - 1;
    let current_replicon_tick = RepliconTick::new(500);
    let stale_replicon_tick = RepliconTick::new(1);

    stepper
        .client_app_mut(0)
        .insert_resource(RollbackObserved::default());
    stepper.client_app_mut(0).add_systems(
        PreUpdate,
        record_rollback
            .after(RollbackSystems::Check)
            .before(RollbackSystems::Prepare),
    );

    let world = stepper.client_world_mut(0);
    world
        .entity_mut(predicted)
        .insert(ConfirmHistory::new(stale_replicon_tick));
    record_completed_mutate_tick(world, current_replicon_tick, server_confirmed_tick);

    stepper.frame_step(1);

    assert!(!stepper.client_world(0).resource::<RollbackObserved>().0);
}

/// Server sends a new component that the client didn't have.
/// On rollback, the component should be present at the confirmed tick.
#[test]
fn a_rollback_applies_a_remote_insert() {
    let (mut stepper, predicted) = setup();

    stepper.frame_step(PROPAGATION_FRAMES);
    let tick = stepper.client_tick(0);
    stepper.frame_step(1);

    // Simulate server sending CompNotNetworked(5.0) at `tick` (component client didn't predict)
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .insert(CompNotNetworked(5.0));
    // Create histories for CompNotNetworked with confirmed value
    let mut confirmed_history = ConfirmedHistory::<CompNotNetworked>::default();
    confirmed_history.insert_present(tick, CompNotNetworked(5.0));
    stepper.client_world_mut(0).entity_mut(predicted).insert((
        PredictionHistory::<CompNotNetworked>::default(),
        confirmed_history,
    ));

    request_forced_rollback_at_completed_tick(&mut stepper, tick);
    stepper.frame_step(1);

    // The remotely-inserted component should be present after rollback
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompNotNetworked>(predicted)
            .unwrap(),
        &CompNotNetworked(5.0),
        "Remotely-inserted component should be present after rollback"
    );
}

/// Server removes a component that the client still had.
/// On rollback, the component should be absent.
#[test]
fn a_rollback_applies_a_remote_removal() {
    let (mut stepper, predicted) = setup();

    stepper.frame_step(1);
    let rollback_tick = stepper.client_tick(0);
    stepper.frame_step(1);

    // Simulate server removing CompFull at rollback_tick
    insert_confirmed::<CompFull>(stepper.client_world_mut(0), predicted, rollback_tick, None);
    stepper
        .client_world_mut(0)
        .entity_mut(predicted)
        .remove::<CompFull>();

    request_forced_rollback_at_completed_tick(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    // CompFull should be absent after rollback: server confirmed removal
    assert!(
        stepper.client_world(0).get::<CompFull>(predicted).is_none(),
        "Remotely-removed component should be absent after rollback"
    );
}

/// If a predicted component has no history sample at-or-before the rollback
/// tick, rollback should not treat that absence as an authoritative removal.
#[test]
fn predicted_component_kept_when_no_history_sample_at_rollback_tick() {
    let (mut stepper, predicted) = setup();

    stepper.frame_step(3);
    let tick = stepper.client_tick(0);

    assert!(
        stepper.client_world(0).get::<CompFull>(predicted).is_some(),
        "precondition: component is present before the rollback"
    );

    {
        let world = stepper.client_world_mut(0);
        let mut history = world
            .get_mut::<PredictionHistory<CompFull>>(predicted)
            .expect("prediction history exists for predicted component");
        history.clear();
        history.add(tick + 5, Some(CompFull(1.0)));
    }

    let rollback_tick = tick - 2;
    trigger_state_rollback(&mut stepper, rollback_tick);
    stepper.frame_step(1);

    assert!(
        stepper.client_world(0).get::<CompFull>(predicted).is_some(),
        "present predicted component should not be removed when no history sample exists at-or-before the rollback tick"
    );
}

/// If we have disable_rollback (LocallyPredicted):
/// 1) the entity alone doesn't trigger rollback
/// 2) if a rollback happens (from another entity), we reset to the predicted history value
#[test]
fn a_locally_predicted_entity_triggers_no_rollback_and_resets_to_its_history() {
    let (mut stepper, predicted_b) = setup();

    // add a LocallyPredicted entity (disable state rollback for it)
    let predicted_a = stepper
        .client_world_mut(0)
        .spawn((Predicted, LocallyPredicted::default(), CompFull(1.0)))
        .id();

    // value gets synced and added to PredictionHistory
    stepper.frame_step(1);

    // 2. If a rollback happens (triggered by predicted_b), LocallyPredicted entity
    //    gets reset to its historical value
    let tick = stepper.client_tick(0);

    // Set up history for predicted_a with a known confirmed value
    insert_confirmed(
        stepper.client_world_mut(0),
        predicted_a,
        tick,
        Some(CompFull(10.0)),
    );

    // Simulate confirmed update for predicted_b with a different value to trigger mismatch
    insert_confirmed(
        stepper.client_world_mut(0),
        predicted_b,
        tick,
        Some(CompFull(3.0)),
    );
    stepper
        .client_world_mut(0)
        .entity_mut(predicted_b)
        .get_mut::<CompFull>()
        .unwrap()
        .0 = 3.0;

    // step once to avoid a 0-tick rollback
    stepper.frame_step(1);

    request_forced_rollback_at_completed_tick(&mut stepper, tick);
    stepper.frame_step(1);

    // the LocallyPredicted entity was rolled back to the past PredictionHistory value
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_a)
            .unwrap()
            .0,
        10.0
    );
    assert_eq!(
        stepper
            .client_world(0)
            .get::<CompFull>(predicted_b)
            .unwrap()
            .0,
        3.0
    );
}

#[derive(Debug, PartialEq)]
struct TimeSnapshot {
    is_rollback: bool,
    delta: Duration,
    elapsed: Duration,
}

#[derive(Resource, Default, Debug)]
struct TimeTracker {
    snapshots: Vec<TimeSnapshot>,
}

// Record the time resource's values for each tick.
fn track_time(
    time: Res<'_, Time>,
    mut time_tracker: ResMut<'_, TimeTracker>,
    rollback: Option<Res<'_, Rollback>>,
) {
    time_tracker.snapshots.push(TimeSnapshot {
        is_rollback: rollback.is_some(),
        delta: time.delta(),
        elapsed: time.elapsed(),
    });
}

/// Test that:
/// - the `Time` resource's elapsed is rollbacked to the first tick of the rollback
/// - the `Time` resource's elapsed time is advanced correctly during the rollback
/// - the `Time` resource's delta during a rollback is the `Time<Fixed>`'s delta
#[test]
fn rollback_rewinds_and_advances_the_time_resource() {
    let (mut stepper, predicted) = setup();
    // Build up enough prediction history so rollback tick is within range
    stepper.frame_step(PROPAGATION_FRAMES);

    // Add time tracking AFTER building history to only capture the rollback frame
    stepper
        .client_app_mut(0)
        .insert_resource(TimeTracker::default());
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, track_time);
    let time_before_next_tick = *stepper.client_world(0).resource::<Time<Fixed>>();

    // Trigger 2 rollback ticks
    let tick = stepper.client_tick(0);
    request_forced_rollback_at_completed_tick(&mut stepper, tick - 2);
    stepper.frame_step(1);

    // Check that the component got synced.
    assert_eq!(
        stepper.client_world(0).get::<CompFull>(predicted).unwrap(),
        &CompFull(1.0)
    );

    // Verify that the 2 rollback ticks and regular tick occurred with the
    // correct delta times and elapsed times.
    let tick_rate = stepper.tick_rate().period();
    let time_tracker = stepper.client_world(0).resource::<TimeTracker>();
    assert_eq!(
        time_tracker.snapshots,
        vec![
            TimeSnapshot {
                is_rollback: true,
                delta: tick_rate,
                elapsed: time_before_next_tick.elapsed() - tick_rate
            },
            TimeSnapshot {
                is_rollback: true,
                delta: tick_rate,
                elapsed: time_before_next_tick.elapsed()
            },
            TimeSnapshot {
                is_rollback: false,
                delta: tick_rate,
                elapsed: time_before_next_tick.elapsed() + tick_rate
            }
        ]
    );
}

/// If we spawn a LocallyPredicted entity with skip_despawn = true
/// We only start enabling rollback for this entity a few ticks after it was spawned.
#[test]
fn a_skip_despawn_entity_joins_rollback_after_its_spawn() {
    let (mut stepper, _) = setup();

    // add predicted/confirmed entities
    let tick = stepper.client_tick(0);
    let predicted_a = stepper
        .client_world_mut(0)
        .spawn((
            Predicted,
            LocallyPredicted {
                skip_despawn: true,
                enable_rollback_after: 2,
            },
            CompFull(1.0),
        ))
        .id();

    // Rollback: the entity should have DisableRollback added until the
    // configured enable_rollback_after tick.
    trigger_state_rollback(&mut stepper, tick);
    stepper.frame_step(1);
    assert!(
        stepper
            .client_world(0)
            .get::<DisableRollback>(predicted_a)
            .is_some()
    );

    // trigger a rollback at tick + 2, we should re-enable rollback
    // since it's the spawn_tick of LocallyPredicted + 2
    trigger_state_rollback(&mut stepper, tick + 2);
    stepper.frame_step(1);
    assert!(
        stepper
            .client_world(0)
            .get::<DisableRollback>(predicted_a)
            .is_none()
    );
}

/// A world with the state the rollback-start systems read, its timeline at tick 20.
fn rollback_state_world() -> World {
    let mut world = World::new();
    world.insert_resource(LocalTimeline::default());
    world.resource_mut::<LocalTimeline>().shift(20);
    world.insert_resource(PredictionSetup::default());
    world.init_resource::<RollbackCheckState>();
    world.init_resource::<PreSpawnedReceiver>();
    world
}

/// A rollback to tick 10 brings back what a prediction despawned after it, and what the server
/// holds whatever its despawn tick; a locally predicted entity despawned at or before it stays
/// disabled.
#[test]
fn a_rollback_undoes_only_the_despawns_it_replays_or_the_server_overrides() {
    let mut world = rollback_state_world();
    let [before, at, after] = [9, 10, 11].map(|tick| {
        world
            .spawn((
                LocallyPredicted::default(),
                PredictionDespawned {
                    tick: NetTick(tick),
                },
            ))
            .id()
    });
    let server_held = world
        .spawn((Predicted, Remote, PredictionDespawned { tick: NetTick(9) }))
        .id();
    world.insert_resource(Rollback(NetTick(10)));

    world
        .run_system_once(RollbackPlugin::remove_prediction_despawned)
        .unwrap();

    let disabled =
        |world: &World, entity: Entity| world.entity(entity).contains::<PredictionDespawned>();
    assert!(disabled(&world, before));
    assert!(disabled(&world, at));
    assert!(!disabled(&world, after));
    assert!(!disabled(&world, server_held));
}

/// The locally predicted lists stay in tick order whatever order their entities spawn in: a
/// longer protection spawned first sorts after a shorter one spawned next.
#[test]
fn the_locally_predicted_lists_stay_in_tick_order() {
    let mut world = rollback_state_world();
    let protected = |after| LocallyPredicted {
        skip_despawn: true,
        enable_rollback_after: after,
    };
    let long = world.spawn(protected(9)).id();
    let short = world.spawn(protected(3)).id();

    assert_eq!(
        world.resource::<RollbackCheckState>().local_skip_despawn,
        [(NetTick(23), short), (NetTick(29), long)]
    );
}

/// A locally predicted entity that spawned before the oldest tick a rollback may restore leaves
/// the despawn list: with a bound of 4 ticks at tick 20, that is tick 16.
#[test]
fn local_despawn_is_pruned_to_the_rollback_window() {
    let mut world = rollback_state_world();
    world
        .resource_mut::<PredictionSetup>()
        .rollback_setup
        .max_rollback_ticks = 4;
    world.insert_resource(InputTimelineSetup::default());
    let entities = [1, 16, 17].map(|tick| (NetTick(tick), world.spawn_empty().id()));
    world.resource_mut::<RollbackCheckState>().local_despawn = entities.to_vec();

    world
        .run_system_once(RollbackPlugin::prune_local_despawn)
        .unwrap();

    assert_eq!(
        world.resource::<RollbackCheckState>().local_despawn,
        entities[1..]
    );
}

/// A timeline switch's rollback keeps a spawn protection that its tick has not passed; a
/// snapshot's rollback ends every one.
#[test]
fn only_a_snapshot_rollback_ends_spawn_protections() {
    for (kind, protected) in [
        (RollbackKind::Switch, true),
        (RollbackKind::Snapshot, false),
    ] {
        let mut world = rollback_state_world();
        let entity = world.spawn_empty().id();
        let mut metadata = world.resource_mut::<RollbackCheckState>();
        metadata.local_skip_despawn = vec![(NetTick(15), entity)];
        metadata.rollback_kind = kind;
        world.insert_resource(Rollback(NetTick(10)));

        world
            .run_system_once(RollbackPlugin::begin_rollback)
            .unwrap();

        assert_eq!(
            world.entity(entity).contains::<DisableRollback>(),
            protected,
            "{kind:?}"
        );
        assert_eq!(
            world
                .resource::<RollbackCheckState>()
                .local_skip_despawn
                .len(),
            usize::from(protected),
            "{kind:?}"
        );
    }
}

#[derive(Debug, Default, Clone)]
struct TestClock;

/// A clock another crate advances in `FixedMain`, as a physics engine's own.
fn advance_test_clock(time: Res<'_, Time>, mut clock: ResMut<'_, Time<TestClock>>) {
    clock.advance_by(time.delta());
}

/// The prediction stack with a foreign clock, registered for local rollback or not.
#[derive(Debug, Clone, Copy)]
struct ForeignClockStack {
    registered: bool,
}

impl campfire_net_stepper::Stack for ForeignClockStack {
    fn add_client(&self, app: &mut App, client: usize, tick_rate: TickRate) {
        PredictionStack.add_client(app, client, tick_rate);
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        PredictionStack.add_server(app, tick_rate);
    }

    fn add_protocol(&self, app: &mut App) {
        PredictionStack.add_protocol(app);
        app.insert_resource(Time::<TestClock>::default());
        if self.registered {
            app.register_component::<Time<TestClock>>().local_rollback();
        }
        app.add_systems(FixedUpdate, advance_test_clock);
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        PredictionStack.client_link(link);
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        PredictionStack.server_link(link);
    }

    fn is_ready(&self, world: &World) -> bool {
        PredictionStack.is_ready(world)
    }
}

/// A rollback rewinds a foreign clock only through its `local_rollback` registration. A frame
/// that rolls back 3 ticks and then simulates one more advances a registered clock by one tick,
/// T, and an unregistered one by the 3 replayed ticks too, 4 T.
#[test]
fn a_foreign_clock_rolls_back_through_its_local_rollback() {
    for (registered, ticks) in [(true, 1), (false, 4)] {
        let mut stepper = Stepper::new(ForeignClockStack { registered }, StepperSetup::single());
        stepper.frame_step(10);
        let elapsed = |stepper: &Stepper<ForeignClockStack>| {
            stepper
                .client_world(0)
                .resource::<Time<TestClock>>()
                .elapsed()
        };
        let before = elapsed(&stepper);
        let tick = stepper.client_tick(0);
        stepper
            .client_app_mut(0)
            .insert_resource(Rollback(tick - 3));
        stepper.frame_step(1);

        assert_eq!(
            elapsed(&stepper) - before,
            ticks * stepper.tick_rate().period(),
            "registered: {registered}"
        );
    }
}
