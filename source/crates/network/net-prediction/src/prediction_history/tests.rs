use super::*;
use crate::prediction_registry::PredictionRegistry;
use campfire_net_replication::ComponentRegistry;
use campfire_net_sync::TimelineSync;
use campfire_net_timeline::Rollback;

use bevy_ecs::entity_disabling::Disabled;

use bevy_ecs::system::RunSystemOnce;

use bevy_replicon::prelude::RepliconTick;

use core::fmt::Debug;

use campfire_net_sync::InputDelaySetup;
use campfire_net_sync::LocalTimelineSync;

use crate::fixtures::prediction_stack::PredictionStack;
use crate::fixtures::protocol::CompCorr;
use crate::fixtures::steps::trigger_state_rollback;
use crate::rollback_setup::RollbackSetup;
use crate::rollback_systems::RollbackSystems;
use bevy::prelude::*;
use bevy_replicon::prelude::Remote;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::NetworkTarget;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::internals::CompA;
use campfire_net_replication::{PredictionTarget, Replicate};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

#[derive(Component, Clone, PartialEq, Debug)]
struct TestValue(f32);

#[test]
fn a_clear_after_a_tick_removes_the_later_predictions() {
    let mut history = PredictionHistory::<TestValue>::default();

    history.add(NetTick(1), Some(TestValue(1.0)));
    history.add(NetTick(5), Some(TestValue(5.0)));
    history.add(NetTick(9), Some(TestValue(9.0)));

    let restore_value = history.get(NetTick(4)).cloned();
    history.clear_after_tick(NetTick(4));

    assert!(matches!(restore_value, Some(TestValue(v)) if v == 1.0));

    let has_tick_5 = history.states().any(|entry| entry.tick == NetTick(5));
    let has_tick_9 = history.states().any(|entry| entry.tick == NetTick(9));
    assert!(!has_tick_5);
    assert!(!has_tick_9);
}

fn prediction_history_test_app(
    max_rollback_ticks: u16,
    input_delay_setup: InputDelaySetup,
    tick: i32,
) -> App {
    let mut app = App::new();
    let mut timeline = LocalTimeline::default();
    timeline.shift(tick);
    app.insert_resource(timeline);
    app.insert_resource(PredictionSetup {
        rollback_setup: RollbackSetup {
            max_rollback_ticks,
            ..Default::default()
        },
        ..Default::default()
    });
    app.insert_resource(InputTimelineSetup::default().with_input_delay(input_delay_setup));
    let mut sync = LocalTimelineSync::default();
    sync.set_synced(true);
    app.insert_resource(sync);
    app
}

#[test]
fn a_prediction_history_records_nothing_before_the_timeline_syncs() {
    let mut app = App::new();
    app.init_resource::<LocalTimeline>();
    app.init_resource::<LocalTimelineSync>();
    app.insert_resource(PredictionSetup::default());
    app.insert_resource(InputTimelineSetup::default());
    app.add_systems(Update, PredictionHistory::<TestValue>::update);
    app.add_observer(PredictionHistory::<TestValue>::record_removal);

    let entity = app
        .world_mut()
        .spawn((TestValue(1.0), PredictionHistory::<TestValue>::default()))
        .id();

    app.update();
    assert!(
        app.world()
            .get::<PredictionHistory<TestValue>>(entity)
            .unwrap()
            .is_empty(),
        "an unsynchronized local tick must not label a prediction sample"
    );

    app.world_mut().entity_mut(entity).remove::<TestValue>();
    assert!(
        app.world()
            .get::<PredictionHistory<TestValue>>(entity)
            .unwrap()
            .is_empty(),
        "an unsynchronized local tick must not label a removal sample"
    );

    app.world_mut()
        .resource_mut::<LocalTimelineSync>()
        .set_synced(true);
    app.world_mut().entity_mut(entity).insert(TestValue(2.0));
    app.update();

    let history = app
        .world()
        .get::<PredictionHistory<TestValue>>(entity)
        .unwrap();
    assert_eq!(history.get(NetTick(0)), Some(&TestValue(2.0)));
}

#[test]
fn a_prediction_history_is_pruned_to_its_rollback_horizon() {
    let mut app = prediction_history_test_app(20, InputDelaySetup::balanced(), 100);
    app.insert_resource(confirmed_through(NetTick(100)));
    app.add_systems(
        Update,
        (
            PredictionHistory::<TestValue>::update,
            PredictionHistory::<TestValue>::prune_confirmed,
        ),
    );

    let mut history = PredictionHistory::default();
    for tick in [80, 90, 95, 100] {
        history.add(NetTick(tick), Some(TestValue(tick as f32)));
    }
    let mut confirmed_history = ConfirmedHistory::default();
    for tick in [80, 90, 95, 100] {
        confirmed_history.insert_present(NetTick(tick), TestValue(tick as f32));
    }
    let entity = app
        .world_mut()
        .spawn((TestValue(100.0), history, confirmed_history))
        .id();

    app.update();

    let history = app
        .world()
        .get::<PredictionHistory<TestValue>>(entity)
        .unwrap();
    assert_eq!(history.states().next().unwrap().tick, NetTick(93));
    assert_eq!(
        history.get(NetTick(93)),
        Some(&TestValue(90.0)),
        "balanced input delay should cap the 20-tick policy at 7 ticks"
    );
    let confirmed_history = app
        .world()
        .get::<ConfirmedHistory<TestValue>>(entity)
        .unwrap();
    assert_eq!(confirmed_history.len(), 3);
}

/// A confirmed tick map whose latest completed confirmed tick is `tick`.
fn confirmed_through(tick: NetTick) -> ConfirmedTickMap {
    let mut confirmed_ticks = ConfirmedTickMap::default();
    let replicon_tick = RepliconTick::new(1);
    confirmed_ticks.record(replicon_tick, tick);
    confirmed_ticks.record_last_confirmed_tick(replicon_tick);
    confirmed_ticks
}

#[test]
fn confirmed_history_is_not_pruned_past_the_latest_confirmed_tick() {
    // Local tick 200, horizon 7, confirmed up to 100: the cut stops at 100, not 193.
    let mut app = prediction_history_test_app(20, InputDelaySetup::balanced(), 200);
    app.insert_resource(confirmed_through(NetTick(100)));
    app.add_systems(Update, PredictionHistory::<TestValue>::prune_confirmed);
    let mut confirmed_history = ConfirmedHistory::default();
    for tick in [80, 90, 95, 100] {
        confirmed_history.insert_present(NetTick(tick), TestValue(tick as f32));
    }
    let entity = app
        .world_mut()
        .spawn((
            TestValue(200.0),
            PredictionHistory::<TestValue>::default(),
            confirmed_history,
        ))
        .id();
    let changed_at = |app: &App| {
        app.world()
            .entity(entity)
            .get_ref::<ConfirmedHistory<TestValue>>()
            .unwrap()
            .last_changed()
    };
    let spawned_at = changed_at(&app);

    app.update();

    assert_eq!(
        changed_at(&app),
        spawned_at,
        "a cut is not an authoritative update, so the history is not changed"
    );
    let ticks = |app: &App| -> Vec<NetTick> {
        let history = app
            .world()
            .get::<ConfirmedHistory<TestValue>>(entity)
            .unwrap();
        history.into_iter().map(|entry| entry.tick).collect()
    };
    assert_eq!(ticks(&app), [NetTick(100)]);
    // A late sample for 120 is then the newest state at 193.
    app.world_mut()
        .get_mut::<ConfirmedHistory<TestValue>>(entity)
        .unwrap()
        .insert_present(NetTick(120), TestValue(120.0));
    let history = app
        .world()
        .get::<ConfirmedHistory<TestValue>>(entity)
        .unwrap();
    assert_eq!(
        history
            .get_state_at_or_before(NetTick(193))
            .and_then(|state| state.value()),
        Some(&TestValue(120.0))
    );
    assert_eq!(ticks(&app), [NetTick(100), NetTick(120)]);

    // No completed confirmed tick: nothing is pruned.
    let mut app = prediction_history_test_app(20, InputDelaySetup::balanced(), 200);
    app.insert_resource(ConfirmedTickMap::default());
    app.add_systems(Update, PredictionHistory::<TestValue>::prune_confirmed);
    let mut confirmed_history = ConfirmedHistory::default();
    confirmed_history.insert_present(NetTick(80), TestValue(80.0));
    let entity = app
        .world_mut()
        .spawn((
            TestValue(200.0),
            PredictionHistory::<TestValue>::default(),
            confirmed_history,
        ))
        .id();
    app.update();
    let history = app
        .world()
        .get::<ConfirmedHistory<TestValue>>(entity)
        .unwrap();
    assert_eq!(
        history.newest_present().map(|entry| entry.tick),
        Some(NetTick(80))
    );
}

/// A late `Predicted` on an entity whose latest confirmation, replicon tick 4, is older than every
/// retained confirmed tick (10, 11 and 12, at ticks 100, 110 and 120) backfills its value at the
/// oldest retained tick, 100: nothing for the entity arrived through those confirmed ticks, so the
/// value held then.
#[test]
fn a_late_predicted_marker_backfills_a_confirmation_older_than_the_window() {
    let mut world = World::new();
    let mut confirmed_ticks = ConfirmedTickMap::default();
    for replicon_tick in 10..=12 {
        confirmed_ticks.record(
            RepliconTick::new(replicon_tick),
            NetTick(replicon_tick * 10),
        );
    }
    world.insert_resource(confirmed_ticks);
    world.add_observer(PredictionHistory::<TestValue>::backfill_confirmed);
    let entity = world
        .spawn((TestValue(5.0), ConfirmHistory::new(RepliconTick::new(4))))
        .id();

    world.entity_mut(entity).insert(Predicted);
    world.flush();

    let history = world.get::<ConfirmedHistory<TestValue>>(entity).unwrap();
    assert_eq!(
        history.newest_present(),
        Some(Ticked::new(NetTick(100), &TestValue(5.0)))
    );
}

#[test]
fn a_snap_to_the_confirmed_value_matches_the_typed_query() {
    const TICK: NetTick = NetTick(10);

    let mut app = App::new();
    app.init_resource::<PredictionRegistry>();
    app.init_resource::<ComponentRegistry>();
    let prediction_history_id = app
        .world_mut()
        .register_component::<PredictionHistory<TestValue>>();
    {
        let mut registry = app.world_mut().resource_mut::<PredictionRegistry>();
        registry.register_rollback::<TestValue>(prediction_history_id);
        registry.register::<TestValue>();
    }

    // The replay simulates the tick after `TICK`, which starts from the state at the end of
    // `TICK`.
    let mut timeline = LocalTimeline::default();
    timeline.shift(TICK.0 as i32 + 1);
    app.insert_resource(timeline);

    let present_history = |value| {
        let mut history = ConfirmedHistory::default();
        history.insert_present(TICK, TestValue(value));
        history
    };

    let changed = app
        .world_mut()
        .spawn((Predicted, TestValue(1.0), present_history(2.0)))
        .id();
    let unchanged = app
        .world_mut()
        .spawn((Predicted, TestValue(3.0), present_history(3.0)))
        .id();
    let inserted = app
        .world_mut()
        .spawn((Predicted, present_history(4.0)))
        .id();

    let mut removed_history = ConfirmedHistory::<TestValue>::default();
    removed_history.insert_removed(TICK);
    let removed = app
        .world_mut()
        .spawn((Predicted, TestValue(5.0), removed_history))
        .id();

    // The old Query obeyed Bevy's default disabling filters. Cached
    // archetypes must exclude the same entities.
    let disabled = app
        .world_mut()
        .spawn((Predicted, Disabled, TestValue(6.0), present_history(7.0)))
        .id();

    app.world_mut().clear_trackers();
    let changed_tick = app
        .world()
        .entity(changed)
        .get_change_ticks::<TestValue>()
        .unwrap()
        .changed;
    let unchanged_tick = app
        .world()
        .entity(unchanged)
        .get_change_ticks::<TestValue>()
        .unwrap()
        .changed;

    app.world_mut()
        .run_system_once(PredictionHistory::<TestValue>::snap_to_confirmed)
        .unwrap();

    assert_eq!(app.world().get::<TestValue>(changed), Some(&TestValue(2.0)));
    assert_ne!(
        app.world()
            .entity(changed)
            .get_change_ticks::<TestValue>()
            .unwrap()
            .changed,
        changed_tick,
        "a changed live value must retain Bevy change detection"
    );
    assert_eq!(
        app.world().get::<TestValue>(unchanged),
        Some(&TestValue(3.0))
    );
    assert_eq!(
        app.world()
            .entity(unchanged)
            .get_change_ticks::<TestValue>()
            .unwrap()
            .changed,
        unchanged_tick,
        "an equal confirmed value must not spuriously mark the component changed"
    );
    assert_eq!(
        app.world().get::<TestValue>(inserted),
        Some(&TestValue(4.0))
    );
    assert!(app.world().get::<TestValue>(removed).is_none());
    assert_eq!(
        app.world().get::<TestValue>(disabled),
        Some(&TestValue(6.0)),
        "default-disabled entities must stay outside the cached query universe"
    );
}

#[test]
fn a_pre_spawned_entity_gets_a_history() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let predicted = stepper
        .client_app_mut(0)
        .world_mut()
        .spawn(CompFull(1.0))
        .id();
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<PredictionHistory<CompFull>>(predicted)
            .is_none()
    );
    stepper.client_fixed_update(0, move |world| {
        world.entity_mut(predicted).insert(PreSpawned::new(0));
    });
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<PredictionHistory<CompFull>>(predicted)
            .is_some()
    );
}

// TODO: test that PredictionHistory is added when a component is added to a PrePredicted or
// PreSpawned entity

#[derive(Debug, Component)]
struct ObservedCompleteInitialPrediction;

fn observe_initial_prediction(
    trigger: On<'_, '_, Add<Predicted>>,
    query: Query<
        '_,
        '_,
        (),
        (
            With<CompFull>,
            With<CompCorr>,
            With<ConfirmedHistory<CompFull>>,
            With<ConfirmedHistory<CompCorr>>,
        ),
    >,
    mut commands: Commands<'_, '_>,
) {
    if query.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(ObservedCompleteInitialPrediction);
    }
}

/// Incoming `PredictedSend` is processed before the other components in the
/// initial update, so every predicted component is inserted live and into
/// confirmed history before observers run.
#[test]
fn an_initial_marker_seeds_live_and_confirmed_history() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    stepper
        .client_app_mut(0)
        .add_observer(observe_initial_prediction);

    // Spawn an entity on the server with a predicted component
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            CompFull(42.0),
            CompCorr(24.0),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    // Let the entity replicate to the client
    stepper.frame_step(2);

    let predicted_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity was not replicated to client");

    // The client entity should have Predicted after receiving PredictedSend.
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Predicted>(predicted_entity)
            .is_some(),
        "client entity should have Predicted marker"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .entity(predicted_entity)
            .contains::<ObservedCompleteInitialPrediction>(),
        "Predicted observers should see live components and confirmed histories from the same update"
    );

    // The client entity should have the CompFull value from the server
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompFull>(predicted_entity)
            .expect("client entity should have CompFull from replication"),
        &CompFull(42.0)
    );
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<CompCorr>(predicted_entity)
            .expect("client entity should have CompCorr from replication"),
        &CompCorr(24.0)
    );

    // Resolve the server tick that produced the init message and check
    // the confirmed history in the same scope.
    let world = stepper.client_app(0).world();
    let prediction_history = world
        .get::<PredictionHistory<CompFull>>(predicted_entity)
        .expect("client entity should have PredictionHistory<CompFull>");
    let confirmed_history = world
        .get::<ConfirmedHistory<CompFull>>(predicted_entity)
        .expect("client entity should have ConfirmedHistory<CompFull>");
    let corr_confirmed_history = world
        .get::<ConfirmedHistory<CompCorr>>(predicted_entity)
        .expect("client entity should have ConfirmedHistory<CompCorr>");
    let confirm = world
        .get::<ConfirmHistory>(predicted_entity)
        .expect("client entity should have ConfirmHistory");
    let confirmed_ticks = world.resource::<ConfirmedTickMap>();
    let s_tick = confirmed_ticks
        .confirmed_state_tick(confirm)
        .expect("confirmed tick map should resolve the last confirm tick");

    // Core assertion: the confirmed history should contain an entry at tick S
    // with the value received from the server (CompFull(42.0)).
    // If write_history didn't fire at init time (because Predicted wasn't
    // visible when markers were checked), the confirmed history would be empty.
    let confirmed_at_s = confirmed_history.get_state_at(s_tick);
    assert!(
        confirmed_at_s.is_some(),
        "ConfirmedHistory should have a confirmed entry at server tick {s_tick:?}, \
         but found history: {confirmed_history:?}"
    );
    assert_eq!(
        corr_confirmed_history
            .get_state_at(s_tick)
            .and_then(HistoryState::value),
        Some(&CompCorr(24.0)),
        "all initially replicated predicted components should use marker receive functions"
    );
    assert!(
        prediction_history
            .states()
            .all(|Ticked { value: state, .. }| {
                matches!(state, HistoryState::Updated(_) | HistoryState::Removed)
            }),
        "PredictionHistory should contain only local predicted states: {prediction_history:?}"
    );
}

#[test]
fn a_manual_predicted_marker_backfills_an_existing_replicated_component() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((CompFull(42.0), Replicate::to_clients(NetworkTarget::All)))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity was not replicated to client");
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<ConfirmedHistory<CompFull>>(client_entity)
            .is_none()
    );

    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(client_entity)
        .insert(Predicted);

    let world = stepper.client_app(0).world();
    let confirm_history = world
        .get::<ConfirmHistory>(client_entity)
        .expect("replicated entity should have ConfirmHistory");
    let server_tick = world
        .resource::<ConfirmedTickMap>()
        .confirmed_state_tick(confirm_history)
        .expect("confirmation tick should resolve to a server tick");
    assert!(
        world
            .get::<PredictionHistory<CompFull>>(client_entity)
            .is_some(),
        "manual prediction should initialize local prediction history"
    );
    assert_eq!(
        world
            .get::<ConfirmedHistory<CompFull>>(client_entity)
            .and_then(|history| history.get_state_at(server_tick))
            .and_then(HistoryState::value),
        Some(&CompFull(42.0)),
        "manual prediction should seed confirmed history from the existing replicated value"
    );
    assert_eq!(world.get::<CompFull>(client_entity), Some(&CompFull(42.0)));
}

#[test]
fn a_manual_predicted_marker_with_its_component_seeds_no_confirmed_history() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((CompA(1.0), Replicate::to_clients(NetworkTarget::All)))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity was not replicated to client");
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(client_entity)
        .insert((Predicted, CompFull(99.0)));

    let entity = stepper.client_app(0).world().entity(client_entity);
    assert!(entity.contains::<PredictionHistory<CompFull>>());
    assert!(
        !entity.contains::<ConfirmedHistory<CompFull>>(),
        "a component added locally with Predicted is not authoritative"
    );
}

#[derive(Debug, Resource, Default)]
struct RollbackObserved(bool);

fn observe_rollback(
    rollback: Option<Res<'_, Rollback>>,
    mut observed: ResMut<'_, RollbackObserved>,
) {
    observed.0 |= rollback.is_some();
}

/// A Full-mode component inserted by the server after the predicted entity was
/// spawned should be detected as a mismatch and applied through rollback.
#[test]
fn a_server_insert_on_a_predicted_entity_starts_a_rollback() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    stepper
        .client_app_mut(0)
        .init_resource::<RollbackObserved>();
    stepper.client_app_mut(0).add_systems(
        PreUpdate,
        observe_rollback
            .after(RollbackSystems::Check)
            .before(RollbackSystems::Prepare),
    );
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(2);
    let predicted_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity was not replicated to the client");
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<CompFull>(predicted_entity)
            .is_none()
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .insert(CompFull(42.0));
    stepper.frame_step(2);

    let world = stepper.client_app(0).world();
    assert!(
        world.resource::<RollbackObserved>().0,
        "the confirmed-tick full scan should detect the authoritative insert"
    );
    assert_eq!(
        world
            .get::<ConfirmedHistory<CompFull>>(predicted_entity)
            .and_then(ConfirmedHistory::newest_present)
            .map(|entry| entry.value),
        Some(&CompFull(42.0)),
        "the authoritative insert should be buffered in confirmed history"
    );
    assert!(
        world
            .get::<PredictionHistory<CompFull>>(predicted_entity)
            .is_some(),
        "the confirmed insert should seed rollback membership for the component"
    );

    assert_eq!(
        world.get::<CompFull>(predicted_entity),
        Some(&CompFull(42.0)),
        "the authoritative insert should trigger a rollback that applies the component"
    );
}

/// When two previously absent components arrive in the same update, the first mismatch suppresses
/// the redundant check for the second. Both still need prediction history so the resulting rollback
/// restores both components.
#[test]
fn two_server_inserts_are_both_applied_by_one_rollback() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    stepper
        .client_app_mut(0)
        .init_resource::<RollbackObserved>();
    stepper.client_app_mut(0).add_systems(
        PreUpdate,
        observe_rollback
            .after(RollbackSystems::Check)
            .before(RollbackSystems::Prepare),
    );
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();

    stepper.frame_step(2);
    let predicted_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity was not replicated to the client");
    let client_entity = stepper.client_app(0).world().entity(predicted_entity);
    assert!(!client_entity.contains::<CompFull>());
    assert!(!client_entity.contains::<CompCorr>());

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .insert((CompFull(42.0), CompCorr(24.0)));
    stepper.frame_step(2);

    let world = stepper.client_app(0).world();
    assert!(
        world.resource::<RollbackObserved>().0,
        "the authoritative inserts should trigger a rollback"
    );
    assert!(
        world
            .get::<PredictionHistory<CompFull>>(predicted_entity)
            .is_some(),
        "the CompFull insert should seed rollback membership"
    );
    assert!(
        world
            .get::<PredictionHistory<CompCorr>>(predicted_entity)
            .is_some(),
        "the CompCorr insert should seed rollback membership even if its mismatch check was suppressed"
    );
    assert_eq!(
        world.get::<CompFull>(predicted_entity),
        Some(&CompFull(42.0))
    );
    assert_eq!(
        world.get::<CompCorr>(predicted_entity),
        Some(&CompCorr(24.0))
    );
}

fn check_history_ticks_ascend(stepper: &Stepper<PredictionStack>, entity: Entity) {
    let history = stepper
        .client_app(0)
        .world()
        .get::<PredictionHistory<CompFull>>(entity)
        .expect("Expected prediction history to be added");
    let ticks: Vec<NetTick> = history.states().map(|entry| entry.tick).collect();
    assert!(
        ticks.is_sorted_by(|earlier, later| earlier < later),
        "History has duplicate or out-of-order ticks: {ticks:?}"
    );
}

/// The history gets updated correctly
/// 1. Updating the predicted component for Comp::Full
/// 2. Removing the predicted component
/// 3. Updating the predicted component during rollback
/// 4. Removing the predicted component during rollback
#[test]
fn a_history_follows_each_update_and_removal_in_and_out_of_a_rollback() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    // add predicted, component
    let _tick = stepper.client_tick(0);
    let predicted = stepper
        .client_app_mut(0)
        .world_mut()
        .spawn((Predicted, Remote))
        .id();

    // 1. Updating Comp::Full on predicted component
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(predicted)
        .insert(CompFull(1.0));
    stepper.frame_step(1);
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(predicted)
        .get_mut::<CompFull>()
        .unwrap()
        .0 = 2.0;
    stepper.frame_step(1);
    let tick = stepper.client_tick(0);
    assert_eq!(
        stepper
            .client_app_mut(0)
            .world_mut()
            .entity_mut(predicted)
            .get_mut::<PredictionHistory<CompFull>>()
            .expect("Expected prediction history to be added")
            .get_state_at_or_before(tick)
            .cloned(),
        Some(HistoryState::Updated(CompFull(2.0))),
        "Expected component value to be updated in prediction history"
    );

    // 2. Removing Comp::Full on predicted entity
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(predicted)
        .remove::<CompFull>();
    stepper.frame_step(1);
    let tick = stepper.client_tick(0);
    assert_eq!(
        stepper
            .client_app_mut(0)
            .world_mut()
            .entity_mut(predicted)
            .get_mut::<PredictionHistory<CompFull>>()
            .expect("Expected prediction history to be added")
            .get_state_at_or_before(tick)
            .cloned(),
        Some(HistoryState::Removed),
        "Expected component value to be removed in prediction history"
    );

    // 3. After rollback, component is restored from history
    // Re-add CompFull and build history so rollback has valid data
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(predicted)
        .insert(CompFull(3.0));
    stepper.frame_step(1);
    let rollback_tick = stepper.client_tick(0);
    stepper.frame_step(1); // advance so there's room for rollback
    trigger_state_rollback(&mut stepper, rollback_tick);
    stepper.frame_step(1);
    assert_eq!(
        stepper
            .client_app_mut(0)
            .world_mut()
            .entity_mut(predicted)
            .get_mut::<PredictionHistory<CompFull>>()
            .expect("Expected prediction history to be added")
            .get_state_at_or_before(rollback_tick)
            .cloned(),
        Some(HistoryState::Updated(CompFull(3.0))),
        "Expected component value to be restored from history during rollback"
    );
    check_history_ticks_ascend(&stepper, predicted);

    // 4. Rollback to middle of history preserves the value at that tick
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(predicted)
        .get_mut::<CompFull>()
        .unwrap()
        .0 = 4.0;
    stepper.frame_step(1);
    let mid_tick = stepper.client_tick(0);
    stepper.frame_step(1);
    trigger_state_rollback(&mut stepper, mid_tick);
    stepper.frame_step(1);
    assert_eq!(
        stepper
            .client_app_mut(0)
            .world_mut()
            .entity_mut(predicted)
            .get_mut::<PredictionHistory<CompFull>>()
            .expect("Expected prediction history to be added")
            .get_state_at_or_before(mid_tick)
            .cloned(),
        Some(HistoryState::Updated(CompFull(4.0))),
        "Expected component value preserved during mid-history rollback"
    );
    check_history_ticks_ascend(&stepper, predicted);
}

/// An add older than the newest entry would leave the history out of order.
#[test]
#[should_panic(expected = "a history is added to in tick order")]
fn an_older_add_panics() {
    let mut history = PredictionHistory::<TestValue>::default();
    history.add(NetTick(5), Some(TestValue(1.0)));
    history.add(NetTick(4), Some(TestValue(2.0)));
}

/// With values at 1, 2 and 4 and a removal at 5, a lookup takes the newest entry at or before
/// its tick; a cut at 3 keeps the state of 2 as the front, at 3, and the iterator yields the
/// present values alone.
#[test]
fn a_history_reads_each_tick_from_the_newest_entry_at_or_before_it() {
    let mut history = PredictionHistory::<TestValue>::default();
    history.add(NetTick(1), Some(TestValue(1.0)));
    history.add(NetTick(2), Some(TestValue(2.0)));
    history.add(NetTick(4), Some(TestValue(4.0)));
    history.add(NetTick(5), None);

    assert_eq!(
        [0, 1, 2, 3, 4, 5].map(|tick| history.get(NetTick(tick)).cloned()),
        [
            None,
            Some(TestValue(1.0)),
            Some(TestValue(2.0)),
            Some(TestValue(2.0)),
            Some(TestValue(4.0)),
            None
        ]
    );
    assert_eq!(
        history.get_state_at_or_before(NetTick(6)),
        Some(&HistoryState::Removed)
    );

    history.clear_until_tick(NetTick(3));
    assert_eq!(
        history.states().map(Ticked::cloned).collect::<Vec<_>>(),
        [
            Ticked::new(NetTick(3), HistoryState::Updated(TestValue(2.0))),
            Ticked::new(NetTick(4), HistoryState::Updated(TestValue(4.0))),
            Ticked::new(NetTick(5), HistoryState::Removed)
        ]
    );
    assert_eq!(
        history
            .iter()
            .map(|entry| (entry.tick, entry.value.clone()))
            .collect::<Vec<_>>(),
        [(NetTick(3), TestValue(2.0)), (NetTick(4), TestValue(4.0))]
    );
}
