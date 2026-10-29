use super::*;
use crate::correction::CorrectionFn;
use crate::fixtures::protocol::linear_correction;
use crate::rollback_plugin::RollbackPlugin;
use crate::timeline_switch::SwitchRejection;
use campfire_log::internals::LogCheck;
use campfire_net_replication::internals::shared_replicon_app;

use crate::correction_ease::CorrectionEase;
use crate::correction_policy::CorrectionPolicy;
use crate::prediction_history::PredictionHistory;
use crate::prediction_marker_plugin::PredictionMarkerPlugin;
use crate::prediction_registration_ext::PredictionRegistrationExt;

use crate::previous_visual::PreviousVisual;

use crate::visual_correction::VisualCorrection;
use bevy_ecs::system::RunSystemOnce;

use campfire_net_core::NetTick;
use campfire_net_frame_interpolation::{
    AppFrameInterpolationExt, FrameHolders, FrameInterpolationHistory, FrameInterpolationPlugin,
    FrameRegistryPlugin, FrameRule, InterpolationSampleCx,
};
use campfire_net_interpolation::InterpolationMarkerPlugin;
use campfire_net_timeline::LocalTimeline;
use core::time::Duration;

use campfire_net_replication::{
    AppComponentExt, ComponentRegistry, ConfirmedHistory, HistoryState,
};
use campfire_net_sync::LocalTimelineSync;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct TestPos(f32);

linear_correction!(TestPos);

/// The predicted component `TestPos` shows, the visual a correction offsets.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct SimPos(f32);

/// The predicted component `TestRot` shows.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct SimRot(f32);

/// A frame-interpolated type with no correction, which a switch still re-seeds.
#[derive(Component, Clone, Debug, Default, PartialEq)]
struct FrameOnly(f32);

/// The visual of `FrameOnly`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
struct ShownFrameOnly(f32);

/// Second corrected type: the example blends up to four (Position,
/// Rotation, velocities), and per-type fan-out needs its own coverage —
/// the duplicate-marker panic proved single-type tests miss it.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct TestRot(f32);

linear_correction!(TestRot);

fn switch_app() -> App {
    let mut app = shared_replicon_app();
    app.add_plugins((
        PredictionMarkerPlugin,
        InterpolationMarkerPlugin,
        FrameRegistryPlugin,
        FrameInterpolationPlugin,
    ));
    app.init_resource::<PredictionRegistry>();
    // The resource `TimelineSwitchPlugin` installs.
    // Requests are components, so there is no message or cursor to register.
    app.init_resource::<TimelineSwitchSetup>();
    app.init_resource::<LocalTimeline>();
    app.init_resource::<LocalTimelineSync>();
    app.insert_resource(Time::<Virtual>::default());
    app.register_component::<SimPos>()
        .replicate()
        .predict()
        .correct_linear::<TestPos, TestPos>();
    app.register_component::<SimRot>()
        .replicate()
        .predict()
        .correct_linear::<TestRot, TestRot>();
    app.register_frame_rule::<SimPos, TestPos, ()>(FrameRule::new(
        |start: &SimPos, end: &SimPos, cx: InterpolationSampleCx| {
            TestPos(start.0 + (end.0 - start.0) * cx.t)
        },
    ));
    app.register_frame_rule::<SimRot, TestRot, ()>(FrameRule::new(
        |start: &SimRot, end: &SimRot, cx: InterpolationSampleCx| {
            TestRot(start.0 + (end.0 - start.0) * cx.t)
        },
    ));
    app.register_frame_rule::<FrameOnly, ShownFrameOnly, ()>(FrameRule::new(
        |_: &FrameOnly, end: &FrameOnly, _| ShownFrameOnly(end.0),
    ));
    app.finish();
    app
}

/// Resolves the archetypes made since the last frame, as each frame does before it writes the
/// visuals, so the correction finds where they are written.
fn refresh_holders(app: &mut App) {
    app.world_mut()
        .run_system_once(FrameHolders::refresh)
        .unwrap();
}

/// Prepares a rollback as `RollbackSystems::Prepare` does: each corrected visual is saved, and each
/// predicted component rewinds to its state at the rollback tick.
fn prepare_rollback(app: &mut App) {
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::save_rollback_visual)
        .unwrap();
    app.world_mut()
        .run_system_once(CorrectionFn::<SimRot, TestRot, TestRot>::save_rollback_visual)
        .unwrap();
    app.world_mut()
        .run_system_once(RollbackPlugin::prepare_rollback::<SimPos>)
        .unwrap();
    app.world_mut()
        .run_system_once(RollbackPlugin::prepare_rollback::<SimRot>)
        .unwrap();
    app.world_mut().flush();
}

/// Runs the `PostUpdate` pass that saves the on-screen value of the
/// requests inserted since the last one.
fn run_save(app: &mut App) {
    app.world_mut()
        .run_system_once(TimelineSwitchPlugin::save_previous_visuals)
        .unwrap();
    app.world_mut().flush();
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::save_switch_visual)
        .unwrap();
    app.world_mut()
        .run_system_once(CorrectionFn::<SimRot, TestRot, TestRot>::save_switch_visual)
        .unwrap();
    app.world_mut().flush();
}

/// Runs the `PreUpdate` pass that applies the saved requests.
fn run_apply(app: &mut App) {
    app.world_mut()
        .run_system_once(TimelineSwitchPlugin::apply_saved_switches)
        .unwrap();
    app.world_mut().flush();
}

/// Runs the `PostUpdate` pass that applies the correction to the live value,
/// which is what makes `live` hold the rendered value.
fn run_correction_apply(app: &mut App) {
    refresh_holders(app);
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::update_visual_correction)
        .unwrap();
    app.world_mut().flush();
}

/// Runs the `PostUpdate` pass that records corrections from the saved
/// values: each correction's system, which is also what a rollback goes through.
fn run_blend(app: &mut App) {
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::create_visual_correction)
        .unwrap();
    app.world_mut()
        .run_system_once(CorrectionFn::<SimRot, TestRot, TestRot>::create_visual_correction)
        .unwrap();
    app.world_mut().flush();
}

/// Inserts one request and runs the two passes, the way the schedule does
/// over two frames: the value is saved at the end of the frame the request
/// is seen, and the markers swap at the start of the next one.
fn send_switch(app: &mut App, entity: Entity, request: TimelineSwitch) {
    app.world_mut().entity_mut(entity).insert(request);
    run_save(app);
    run_apply(app);
}

/// Shared blend window stamped on `entity` by the switch handler.
fn blend_marker(world: &World, entity: Entity) -> SwitchBlend {
    *world.get::<SwitchBlend>(entity).expect("blend marker")
}

#[test]
fn a_switch_to_predicted_swaps_the_markers_and_saves_the_visual() {
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(10.0), TestPos(10.0), Interpolated))
        .id();
    app.world_mut().flush();

    let request = TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500));
    send_switch(&mut app, entity, request);

    let world = app.world();
    assert!(world.get::<Predicted>(entity).is_some());
    assert!(world.get::<Interpolated>(entity).is_none());
    // A switch to prediction renders through frame interpolation even
    // though the renderer observer (On<Add<Position>>) never re-fires when
    // the markers swap.
    assert!(world.get::<FrameInterpolate>(entity).is_some());
    // Nothing is saved in this direction: the forced rollback captures the
    // same value from the same live component before restoring it.
    assert!(
        world
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
    // The window is live and the error for this frame has not been written
    // yet: the blend pass runs later in the frame.
    assert!(world.get::<SwitchBlend>(entity).is_some());
    assert!(
        world
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
    assert_eq!(
        blend_marker(world, entity).total(),
        Duration::from_millis(500)
    );
}

#[test]
fn a_switch_to_interpolated_swaps_the_markers_and_saves_the_visual() {
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(4.0), TestPos(4.0), Predicted, FrameInterpolate))
        .id();
    app.world_mut().flush();

    send_switch(
        &mut app,
        entity,
        TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(250)),
    );

    let world = app.world();
    assert!(world.get::<Interpolated>(entity).is_some());
    assert!(world.get::<Predicted>(entity).is_none());
    // Delayed interpolation holds the pose from here on; frame
    // interpolation would freeze the render on its stale value.
    assert!(world.get::<FrameInterpolate>(entity).is_none());
    assert_eq!(
        world
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .cloned(),
        Some(TestPos(4.0))
    );
    assert!(world.get::<SwitchBlend>(entity).is_some());
}

/// The blend writes one error per frame: `(saved - destination)` scaled by
/// how much of the window is left, so the rendered value is a lerp from the
/// value on screen at the switch to whatever the destination has now. The
/// live value is left alone; the shared apply adds the error afterwards.
#[test]
fn blend_waits_for_a_component_the_destination_has_not_presented() {
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(10.0), TestPos(10.0), Predicted, FrameInterpolate))
        .id();
    app.world_mut().flush();

    send_switch(
        &mut app,
        entity,
        TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)),
    );
    app.world_mut().entity_mut(entity).remove::<TestPos>();
    app.world_mut().flush();
    assert!(
        app.world()
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_some(),
        "the saved value must outlive the component it was read from"
    );

    run_blend(&mut app);
    assert!(
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none(),
        "nothing to blend without a destination value"
    );

    // A snapshot arrives with the delayed value; the gap is measured now.
    app.world_mut().entity_mut(entity).insert(TestPos(16.0));
    app.world_mut().flush();
    run_blend(&mut app);
    assert_eq!(
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestPos(-6.0))
    );
}

/// A blend's error decays on its window's curve, not on the entity's
/// correction policy: the length and curve the caller asked for are what the
/// transition looks like. An unbounded curve keeps that shape through the
/// window too — its per-frame keep ratio is constant, so the window bounds it
/// by its length rather than by normalising it.
#[test]
fn blend_error_decays_on_the_window_curve() {
    use crate::prediction_setup::PredictionSetup;
    use bevy_ecs::system::RunSystemOnce;

    let mut app = switch_app();
    app.insert_resource(PredictionSetup::default());
    // Same offset, one with a window and one without. Early in a one second
    // smoothstep window the curve gives up almost nothing, while the policy's
    // 200 ms half-life is already shrinking it.
    let blended = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Predicted,
            SwitchBlend::new(
                Duration::ZERO,
                Duration::from_secs(1),
                CorrectionEase::Smoothstep,
            ),
            VisualCorrection::active(TestPos(-1.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    let policy = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Predicted,
            VisualCorrection::active(TestPos(-1.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    // Slower than the policy's own exponential: the window and the policy
    // combine by taking whichever gives the error up more slowly, so only a
    // curve that is slower than the floor can be the one observed here. (A
    // faster one would be floored, and the assertion below could not tell the
    // two exponential periods apart.)
    let exponential = CorrectionEase::Exponential {
        decay_ratio: 0.5,
        decay_period: Duration::from_millis(400),
    };
    let unbounded = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Predicted,
            SwitchBlend::new(Duration::ZERO, Duration::from_secs(1), exponential),
            VisualCorrection::active(TestPos(-1.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    app.world_mut().flush();

    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(50));
    refresh_holders(&mut app);
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::update_visual_correction)
        .unwrap();
    app.world_mut().flush();

    let error = |entity| {
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.0)
            .unwrap()
    };
    let blend_error = error(blended);
    let policy_error = error(policy);
    assert!(
        blend_error < policy_error,
        "the window's curve should hold the error longer than the policy, \
         got blend {blend_error} and policy {policy_error}"
    );
    // The unbounded curve has no ramp to advance along, so one step of it is
    // exactly the fraction its own period gives up in 50 ms — and that is
    // visible here only because the window's period is slower than the
    // policy floor.
    assert!(
        (error(unbounded)
            - -exponential.remaining(Duration::from_millis(50), Duration::from_secs(1)))
        .abs()
            < 1e-5,
        "an unbounded window keeps its constant ratio, got {}",
        error(unbounded)
    );
}

/// The curve never gives an error up faster than the entity's own correction
/// would. Its tail is steep, so without that floor a jump arriving late in the
/// window would be dumped into a single frame.
#[test]
fn window_curve_never_outpaces_the_correction_policy() {
    use crate::prediction_setup::PredictionSetup;
    use bevy_ecs::system::RunSystemOnce;

    let mut app = switch_app();
    app.insert_resource(PredictionSetup::default());
    // The blend starts at 0; the correction 479 ms later, as a rollback arrives late in the
    // window.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(479));
    let entity = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Predicted,
            // 99% through a half second window: the curve alone would drop
            // almost the whole error in this one frame.
            SwitchBlend::new(
                Duration::ZERO,
                Duration::from_millis(500),
                CorrectionEase::Smoothstep,
            ),
            VisualCorrection::active(TestPos(-10.0), Duration::from_millis(479)),
        ))
        .id();
    app.world_mut().flush();
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(16));

    refresh_holders(&mut app);

    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::update_visual_correction)
        .unwrap();
    app.world_mut().flush();

    let error = app
        .world()
        .get::<VisualCorrection<TestPos>>(entity)
        .and_then(|slot| slot.0.as_ref())
        .map(|c| c.error.0)
        .unwrap();
    assert!(
        error < -9.0,
        "the policy floor should have kept most of the error, got {error}"
    );
}

/// Sets up the resources a rollback frame needs, with the rollback target at
/// tick 5 and the local timeline at tick 10.
fn rollback_env(app: &mut App) {
    use crate::prediction_setup::PredictionSetup;
    use campfire_net_timeline::Rollback;

    app.init_resource::<LocalTimeline>();
    app.init_resource::<ComponentRegistry>();
    app.insert_resource(PredictionSetup::default());
    app.insert_resource(Rollback(NetTick(5)));
    app.world_mut().resource_mut::<LocalTimeline>().shift(10);
}

/// History with a confirmed and a predicted value at `tick`.
fn history_at(tick: u32, value: f32) -> (PredictionHistory<SimPos>, ConfirmedHistory<SimPos>) {
    let mut predicted = PredictionHistory::<SimPos>::default();
    predicted.add(NetTick(tick), Some(SimPos(value)));
    let mut confirmed = ConfirmedHistory::<SimPos>::default();
    confirmed.insert(NetTick(tick), HistoryState::Updated(SimPos(value)));
    (predicted, confirmed)
}

/// Runs one rollback frame with no switch involved: rewind, replay to
/// `replayed`, and let the rollback path compute the correction.
fn rollback_frame(app: &mut App, replayed: f32) {
    prepare_rollback(app);
    app.world_mut().flush();
    let entities: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<PredictionHistory<SimPos>>>()
        .iter(app.world())
        .collect();
    for entity in entities {
        if let Some(mut position) = app.world_mut().get_mut::<SimPos>(entity) {
            position.0 = replayed;
        }
        if let Some(mut shown) = app.world_mut().get_mut::<TestPos>(entity) {
            shown.0 = replayed;
        }
    }
    run_blend(app);
    app.world_mut().flush();
    // The blend pass is where a switch window computes its correction; it is
    // a no-op for entities without one.
    run_blend(app);
    run_correction_apply(app);
}

/// Switching to the interpolation timeline while a rollback correction is
/// still decaying: the value to blend from has to be the render, which
/// already carries that correction, and the new measurement replaces it.
#[test]
fn switch_to_interpolated_while_a_correction_is_in_progress() {
    use bevy_time::Fixed;

    let mut app = switch_app();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    rollback_env(&mut app);
    let (history, confirmed) = history_at(5, 5.0);
    let entity = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Predicted,
            FrameInterpolate,
            history,
            confirmed,
            FrameInterpolationHistory::<SimPos> {
                previous_value: Some(SimPos(5.0)),
                current_value: Some(SimPos(10.0)),
            },
        ))
        .id();
    app.world_mut().flush();

    // A rollback puts a correction in flight: the render holds at 10 while
    // the simulation has moved to 20.
    rollback_frame(&mut app, 20.0);
    // The rollback holds the render somewhere between the pre-rollback
    // render and the replayed value; what matters here is that a correction
    // is in flight and that the switch measures from the render.
    let render = app.world().get::<TestPos>(entity).unwrap().0;
    let in_flight = app
        .world()
        .get::<VisualCorrection<TestPos>>(entity)
        .and_then(|slot| slot.0.as_ref())
        .map(|c| c.error.0);
    assert!(
        in_flight.is_some_and(|error| error.abs() > 0.5),
        "the rollback should have left a correction, got {in_flight:?}"
    );

    // Now switch to the interpolation timeline.
    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    run_apply(&mut app);
    // The destination timeline writes its first value.
    app.world_mut().get_mut::<TestPos>(entity).unwrap().0 = 30.0;
    run_blend(&mut app);

    assert_eq!(
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestPos(render - 30.0)),
        "the switch must measure from the render it interrupted"
    );
}

/// The forced rollback asks for a rewind to the last processed confirmed
/// tick. When the entity's confirmed history starts *after* that tick there
/// is no state to restore, so the rollback captures nothing — and the switch
/// to the prediction timeline is then the only thing that can record what to
/// blend from.
#[test]
fn forced_rollback_that_finds_no_state_can_still_blend() {
    use bevy_time::Fixed;

    let mut app = switch_app();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    rollback_env(&mut app);
    // Confirmed data at tick 8 only: the rollback target is tick 5.
    let (history, confirmed) = history_at(8, 8.0);
    let entity = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Interpolated,
            history,
            confirmed,
        ))
        .id();
    app.world_mut().flush();

    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    run_apply(&mut app);
    rollback_frame(&mut app, 30.0);

    let error = app
        .world()
        .get::<VisualCorrection<TestPos>>(entity)
        .and_then(|slot| slot.0.as_ref())
        .map(|c| c.error.clone());
    assert_eq!(
        error,
        Some(TestPos(10.0 - 30.0)),
        "a switch must still blend when the rollback finds no state to restore"
    );
}

/// A switch to the prediction timeline does not save anything: the forced
/// rollback is the capture, and it takes the value from the same live
/// component before restoring it. This pins that the rollback really is
/// enough, since nothing else records the value in this direction.
#[test]
fn forced_rollback_captures_the_value_a_switch_blends_from() {
    use crate::prediction_history::PredictionHistory;
    use crate::prediction_setup::PredictionSetup;
    use campfire_net_replication::ComponentRegistry;
    use campfire_net_timeline::LocalTimeline;
    use campfire_net_timeline::Rollback;

    let mut app = switch_app();
    app.init_resource::<LocalTimeline>();
    app.init_resource::<ComponentRegistry>();
    app.insert_resource(PredictionSetup::default());
    app.insert_resource(Rollback(NetTick(5)));
    app.world_mut().resource_mut::<LocalTimeline>().shift(10);
    let mut history = PredictionHistory::<SimPos>::default();
    history.add(NetTick(5), Some(SimPos(5.0)));
    let mut confirmed = ConfirmedHistory::<SimPos>::default();
    confirmed.insert(NetTick(5), HistoryState::Updated(SimPos(5.0)));
    // Interpolated and showing 10.
    let entity = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Interpolated,
            history,
            confirmed,
        ))
        .id();
    app.world_mut().flush();

    // Frame F: the switch saves nothing in this direction.
    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    assert!(
        app.world()
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );

    // Frame F+1: the markers swap, then the rollback captures.
    run_apply(&mut app);
    prepare_rollback(&mut app);
    app.world_mut().flush();

    // The rollback is the capture in this direction, and it takes the value
    // the entity was rendering.
    assert_eq!(
        app.world()
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .cloned(),
        Some(TestPos(10.0)),
        "the rollback must capture the value the switch blends from"
    );
    assert_eq!(
        app.world().get::<SimPos>(entity),
        Some(&SimPos(5.0)),
        "... while it rewinds the simulated value to the confirmed state"
    );
}

/// Drives a whole frame the way the schedule does around a switch to the
/// prediction timeline: the rollback in `PreUpdate`, replay, then the switch
/// correction pass in `PostUpdate`.
fn frame_with_rollback(app: &mut App, replayed: f32) {
    use crate::prediction_setup::PredictionSetup;
    use campfire_net_timeline::Rollback;

    app.world_mut().init_resource::<ComponentRegistry>();
    app.world_mut().init_resource::<PredictionSetup>();
    app.world_mut().insert_resource(Rollback(NetTick(5)));
    app.world_mut().resource_mut::<LocalTimeline>().shift(10);
    app.world_mut().flush();

    prepare_rollback(app);
    app.world_mut().flush();
    // Replay steps the simulation forward to the replayed value.
    let entities: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<PredictionHistory<SimPos>>>()
        .iter(app.world())
        .collect();
    for entity in entities {
        if let Some(mut position) = app.world_mut().get_mut::<SimPos>(entity) {
            position.0 = replayed;
        }
        if let Some(mut shown) = app.world_mut().get_mut::<TestPos>(entity) {
            shown.0 = replayed;
        }
    }
    run_blend(app);
    app.world_mut().flush();
    run_blend(app);
    // The apply then adds the correction to the live value, so `live` is the
    // render from here on.
    run_correction_apply(app);
}

/// A rollback during a switch to the prediction timeline must be measured
/// against the render it interrupted, like any other rollback. The window is
/// still open, so the rollback path leaves the correction to the blend; the
/// saved value it writes is what the blend's next computation reads.
#[test]
fn rollback_during_a_to_predicted_blend_updates_the_correction() {
    use crate::prediction_history::PredictionHistory;
    use bevy_time::Fixed;
    use campfire_net_timeline::LocalTimeline;

    let mut app = switch_app();
    app.init_resource::<LocalTimeline>();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    // Interpolated, showing 10, with history at the tick the rollbacks will
    // rewind to.
    let mut history = PredictionHistory::<SimPos>::default();
    history.add(NetTick(5), Some(SimPos(5.0)));
    let mut confirmed = ConfirmedHistory::<SimPos>::default();
    confirmed.insert(NetTick(5), HistoryState::Updated(SimPos(5.0)));
    let entity = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Interpolated,
            history,
            confirmed,
        ))
        .id();
    app.world_mut().flush();

    // Frame F: save the value on screen. F+1: swap the markers, roll back,
    // and let the blend compute its correction.
    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    run_apply(&mut app);
    frame_with_rollback(&mut app, 30.0);

    // The apply bakes the correction into the live value, so from here the
    // live value is what renders, and the correction is the offset still to
    // be given up against the destination.
    let held = app.world().get::<TestPos>(entity).unwrap().0;
    assert_eq!(
        held, 10.0,
        "the switch frame should hold the render at the saved value"
    );

    // Mid-window: another rollback moves the destination again. The blend has
    // to re-measure from the render it interrupted, not keep the old offset.
    frame_with_rollback(&mut app, 100.0);

    let error = app
        .world()
        .get::<VisualCorrection<TestPos>>(entity)
        .and_then(|slot| slot.0.as_ref())
        .map(|c| c.error.0)
        .unwrap();
    let render = app.world().get::<TestPos>(entity).unwrap().0;
    assert_eq!(
        render, held,
        "the render must stay where the blend held it (correction now {error})"
    );
    assert!(
        error < -70.0,
        "the correction must be re-measured against the new destination, got {error}"
    );
}

/// A switch takes over both the offset and the schedule of a correction that
/// was in flight when it was applied: the value on screen becomes the blend's
/// starting point, and the window's length governs from then on, whatever
/// tuning the old correction had.
#[test]
fn switch_takes_over_an_in_flight_correction() {
    use crate::prediction_setup::PredictionSetup;
    use bevy_ecs::system::RunSystemOnce;

    let mut app = switch_app();
    app.insert_resource(PredictionSetup::default());
    // Mid-correction: the destination timeline has written 12 and an error of
    // -2 from an earlier rollback is being decayed, so 10 is on screen.
    let entity = app
        .world_mut()
        .spawn((
            SimPos(12.0),
            TestPos(12.0),
            Predicted,
            FrameInterpolate,
            VisualCorrection::active(TestPos(-2.0), Duration::ZERO),
        ))
        .id();
    app.world_mut().flush();
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(16));
    refresh_holders(&mut app);
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::update_visual_correction)
        .unwrap();
    app.world_mut().flush();
    // What the entity is showing now, error included.
    let on_screen = app.world().get::<TestPos>(entity).unwrap().0;

    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    run_apply(&mut app);
    app.world_mut().get_mut::<TestPos>(entity).unwrap().0 = 16.0;
    run_blend(&mut app);

    let world = app.world();
    // The offset is measured from what was on screen, and the window holds it
    // now: the old correction's tuning no longer applies.
    assert_eq!(
        world
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestPos(on_screen - 16.0))
    );
    assert!(
        on_screen > 10.0 && on_screen < 12.0,
        "the old correction was partway through decaying, got {on_screen}"
    );
    // The saved value was consumed: the window holds the error now.
    assert!(
        world
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
    assert!(world.get::<SwitchBlend>(entity).is_some());
}

/// The switch records one jump — the gap between the value on screen and
/// the destination's value — and the ordinary decay gives it up. Same shape
/// as a rollback correction: measure once, then shrink.
#[test]
fn blend_records_one_jump_and_decays_it() {
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(10.0), TestPos(10.0), Predicted, FrameInterpolate))
        .id();
    app.world_mut().flush();

    send_switch(
        &mut app,
        entity,
        TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)),
    );
    // The destination timeline writes its first value between the apply and
    // the seed.
    app.world_mut().get_mut::<TestPos>(entity).unwrap().0 = 16.0;
    run_blend(&mut app);

    // error = saved - destination, so the render holds at the saved value.
    assert_eq!(
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestPos(-6.0))
    );

    // A second seed pass must not record the jump again: the offset is
    // already there and belongs to the window now.
    app.world_mut().get_mut::<TestPos>(entity).unwrap().0 = 20.0;
    run_blend(&mut app);
    assert_eq!(
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestPos(-6.0)),
        "the blend seeds once, it does not re-measure every frame"
    );
}

/// A rollback that lands while the window is open keeps its own hold, and
/// the blend decays that hold instead of re-deriving an offset from the saved
/// value. Re-deriving is what leaks: the offset would be scaled by how much
/// of the window is left, which is nearly zero late in the window, so the
#[test]
fn switch_to_interpolated_saves_one_value_per_corrected_type() {
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((
            SimPos(4.0),
            TestPos(4.0),
            TestRot(1.0),
            Predicted,
            FrameInterpolate,
        ))
        .id();
    app.world_mut().flush();

    send_switch(&mut app, entity, TimelineSwitch::to_interpolated());

    let world = app.world();
    assert_eq!(
        world
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .cloned(),
        Some(TestPos(4.0))
    );
    assert_eq!(
        world
            .get::<PreviousVisual<TestRot>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .cloned(),
        Some(TestRot(1.0))
    );

    // Both destinations write, and both errors follow.
    app.world_mut().get_mut::<TestPos>(entity).unwrap().0 = 10.0;
    app.world_mut().get_mut::<TestRot>(entity).unwrap().0 = 3.0;
    run_blend(&mut app);
    assert_eq!(
        app.world()
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestPos(-6.0))
    );
    assert_eq!(
        app.world()
            .get::<VisualCorrection<TestRot>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|c| c.error.clone()),
        Some(TestRot(-2.0))
    );
}

/// The frame history belongs to the era the entity is leaving, and neither
/// direction can keep it: while interpolated, frame interpolation is not
/// recording it (it only runs for archetypes with `FrameInterpolate`), and
/// once interpolated, frame interpolation stops reading it. Keeping a stale
/// one would let the next era's first frames blend from it. Every frame
/// rule's history goes, a corrected type's or not, on a blending switch or a
/// snapping one.
#[test]
fn switch_drops_the_frame_history_in_both_directions() {
    let stale_histories = || {
        (
            FrameInterpolationHistory::<SimPos> {
                previous_value: Some(SimPos(47.0)),
                current_value: Some(SimPos(47.7)),
            },
            FrameInterpolationHistory::<FrameOnly> {
                previous_value: Some(FrameOnly(47.0)),
                current_value: Some(FrameOnly(47.7)),
            },
        )
    };
    let assert_dropped = |app: &App, entity: Entity, message: &str| {
        let entity = app.world().entity(entity);
        assert!(
            !entity.contains::<FrameInterpolationHistory<SimPos>>()
                && !entity.contains::<FrameInterpolationHistory<FrameOnly>>(),
            "{message}"
        );
    };
    for request in [
        TimelineSwitch::to_predicted(),
        TimelineSwitch::to_predicted().with_transition(Duration::ZERO),
    ] {
        let mut app = switch_app();
        let entity = app
            .world_mut()
            .spawn((
                SimPos(10.0),
                TestPos(10.0),
                FrameOnly(10.0),
                Interpolated,
                stale_histories(),
            ))
            .id();
        send_switch(&mut app, entity, request);
        assert_dropped(
            &app,
            entity,
            "a switch to prediction must drop the stale histories",
        );
    }
    for request in [
        TimelineSwitch::to_interpolated(),
        TimelineSwitch::to_interpolated().with_transition(Duration::ZERO),
    ] {
        let mut app = switch_app();
        let entity = app
            .world_mut()
            .spawn((
                SimPos(10.0),
                TestPos(10.0),
                FrameOnly(10.0),
                Predicted,
                FrameInterpolate,
                stale_histories(),
                PredictionHistory::<SimPos>::default(),
            ))
            .id();
        send_switch(&mut app, entity, request);
        assert_dropped(
            &app,
            entity,
            "a switch to interpolation must not leave the predicted era's histories",
        );
        assert!(
            !app.world()
                .entity(entity)
                .contains::<PredictionHistory<SimPos>>(),
            "an interpolated entity keeps no prediction history to record into and prune from"
        );
    }
}

#[test]
fn a_snap_flips_with_no_blend() {
    {
        let mut app = switch_app();
        let entity = app
            .world_mut()
            .spawn((SimPos(1.0), TestPos(1.0), Interpolated))
            .id();
        app.world_mut().flush();

        send_switch(
            &mut app,
            entity,
            TimelineSwitch::to_predicted().with_transition(Duration::ZERO),
        );

        let world = app.world();
        assert!(world.get::<Predicted>(entity).is_some());
        assert!(world.get::<Interpolated>(entity).is_none());
        assert!(
            world
                .get::<PreviousVisual<TestPos>>(entity)
                .and_then(|slot| slot.0.as_ref())
                .is_none()
        );
        assert!(world.get::<SwitchBlend>(entity).is_none());
    }
}

/// The contract every window curve is used through: whole at the start,
/// converged at the end, never growing in between. A curve ending above zero
/// would leave a permanent offset behind, and one that grew would make the
/// render move away from its destination.
#[test]
fn each_window_curve_starts_whole_ends_converged_and_never_grows() {
    // Bounded curves are normalised over the window they run in.
    for ease in [
        CorrectionEase::Linear,
        CorrectionEase::Smoothstep,
        CorrectionEase::EaseOutCubic,
        CorrectionEase::EaseInOutCubic,
    ] {
        let window = Duration::from_secs(1);
        assert_eq!(
            ease.remaining(Duration::ZERO, window),
            1.0,
            "{ease:?} starts whole"
        );
        assert_eq!(
            ease.remaining(window, window),
            0.0,
            "{ease:?} ends converged"
        );
        let mut prev = f32::INFINITY;
        for p in (0..=1000).step_by(50).map(Duration::from_millis) {
            let r = ease.remaining(p, window);
            assert!(r <= prev, "{ease:?} must not grow at {p:?}");
            prev = r;
        }
    }
}

/// The request's own overrides win; what is left resolves against
/// [`TimelineSwitchSetup`] when the request is saved, so a game retunes the
/// feel in one place and can still tune a single switch.
#[test]
fn a_request_resolves_its_overrides_then_the_setup() {
    let mut app = switch_app();
    app.world_mut().insert_resource(TimelineSwitchSetup {
        default_transition: Duration::from_secs(2),
        default_ease: CorrectionEase::Smoothstep,
    });
    let bare = app
        .world_mut()
        .spawn((SimPos(1.0), TestPos(1.0), Interpolated))
        .id();
    let overridden = app
        .world_mut()
        .spawn((SimPos(2.0), TestPos(2.0), Interpolated))
        .id();
    app.world_mut().flush();

    send_switch(&mut app, bare, TimelineSwitch::to_predicted());
    send_switch(
        &mut app,
        overridden,
        TimelineSwitch::to_predicted()
            .with_transition(Duration::from_millis(500))
            .with_ease(CorrectionEase::EaseOutCubic),
    );

    let world = app.world();
    assert_eq!(blend_marker(world, bare).total(), Duration::from_secs(2));
    assert_eq!(blend_marker(world, bare).ease(), CorrectionEase::Smoothstep);
    assert_eq!(
        blend_marker(world, overridden).total(),
        Duration::from_millis(500)
    );
    assert_eq!(
        blend_marker(world, overridden).ease(),
        CorrectionEase::EaseOutCubic
    );
}

/// The exponential is the unbounded curve: it never reaches zero inside its
/// window and nothing is left from the window's end, its keep ratio is constant
/// inside the window (so it does not depend on where in the curve we are), and
/// it matches the correction policy's own lerp ratio.
#[test]
fn exponential_ease_matches_the_correction_policy() {
    let policy = CorrectionPolicy::new(0.5, Duration::from_millis(200));
    let ease = policy.ease();
    let (window, ms) = (Duration::from_secs(1), Duration::from_millis);
    assert!(matches!(ease, CorrectionEase::Exponential { .. }));
    for dt in [
        Duration::from_millis(8),
        Duration::from_millis(16),
        Duration::from_millis(50),
    ] {
        assert!(
            (ease.keep(Duration::ZERO, dt, window) - policy.lerp_ratio(Duration::ZERO, dt)).abs()
                < 1e-6,
            "the ease must reproduce the policy's own ratio"
        );
    }
    assert!(
        ease.remaining(ms(990), window) > 0.0,
        "an exponential never reaches zero inside its window"
    );
    assert_eq!(ease.remaining(window, window), 0.0, "the window ends it");
    assert_eq!(
        ease.keep(ms(990), ms(16), window),
        0.0,
        "the frame that reaches the window's end gives up the rest"
    );
    // One decay period in, half the error is left.
    assert!((ease.remaining(ms(200), window) - 0.5).abs() < 1e-6);
}

#[test]
fn an_invalid_request_is_dropped() {
    let mut app = switch_app();
    let predicted = app
        .world_mut()
        .spawn((SimPos(1.0), TestPos(1.0), Predicted))
        .id();
    let interpolated = app
        .world_mut()
        .spawn((SimPos(2.0), TestPos(2.0), Interpolated))
        .id();
    let plain = app.world_mut().spawn((SimPos(3.0), TestPos(3.0))).id();
    app.world_mut().flush();

    // Wrong current mode (and a mode-less entity): nothing switches, no
    // blend, and each drop logs why.
    let scoped = LogCheck::scoped(|| {
        send_switch(
            &mut app,
            predicted,
            TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)),
        );
        send_switch(
            &mut app,
            interpolated,
            TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)),
        );
        send_switch(
            &mut app,
            plain,
            TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)),
        );
        // A flip has nothing to flip from there either: it resolves against the
        // entity's markers, and there are none.
        send_switch(&mut app, plain, TimelineSwitch::default());
    });
    let refused = |entity: Entity, rejection| SwitchRefused {
        entity: entity.to_bits(),
        rejection,
    };
    assert_eq!(
        scoped.check.take::<SwitchRefused>(),
        [
            refused(predicted, SwitchRejection::NotInterpolated),
            refused(interpolated, SwitchRejection::NotPredicted),
            refused(plain, SwitchRejection::OnNeither),
            refused(plain, SwitchRejection::OnNeither),
        ]
    );

    let world = app.world();
    assert!(world.get::<Predicted>(predicted).is_some());
    assert!(world.get::<SwitchBlend>(predicted).is_none());
    assert!(world.get::<Interpolated>(interpolated).is_some());
    assert!(world.get::<SwitchBlend>(interpolated).is_none());
    assert!(world.get::<SwitchBlend>(plain).is_none());
    // A dropped request does not stay on the entity and fire later.
    assert!(world.get::<TimelineSwitch>(plain).is_none());

    // A second request while blending is ignored.
    send_switch(
        &mut app,
        interpolated,
        TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)),
    );
    {
        let world = app.world();
        assert!(world.get::<Predicted>(interpolated).is_some());
        assert!(world.get::<SwitchBlend>(interpolated).is_some());
    }
    send_switch(
        &mut app,
        interpolated,
        TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)),
    );
    let world = app.world();
    assert!(world.get::<Predicted>(interpolated).is_some());
    assert!(world.get::<Interpolated>(interpolated).is_none());
    assert!(world.get::<SwitchBlend>(interpolated).is_some());
}

/// A bare `TimelineSwitch::default()` carries no direction: the save pass
/// picks the timeline the entity is not on, so the same value works in both
/// directions and callers that only care about toggling never name one.
#[test]
fn default_flips_the_entity_to_the_other_timeline() {
    let mut app = switch_app();
    let to_predicted = app
        .world_mut()
        .spawn((SimPos(1.0), TestPos(1.0), Interpolated))
        .id();
    let to_interpolated = app
        .world_mut()
        .spawn((SimPos(2.0), TestPos(2.0), Predicted, FrameInterpolate))
        .id();
    app.world_mut().flush();

    send_switch(
        &mut app,
        to_predicted,
        TimelineSwitch::default().with_transition(Duration::from_millis(500)),
    );
    send_switch(
        &mut app,
        to_interpolated,
        TimelineSwitch::default().with_transition(Duration::from_millis(500)),
    );

    let world = app.world();
    assert!(world.get::<Predicted>(to_predicted).is_some());
    assert!(world.get::<Interpolated>(to_predicted).is_none());
    assert!(world.get::<Interpolated>(to_interpolated).is_some());
    assert!(world.get::<Predicted>(to_interpolated).is_none());
    // Both flips blend, so both entities carry the window the save pass
    // resolved from the override.
    assert_eq!(
        blend_marker(world, to_predicted).total(),
        Duration::from_millis(500)
    );
    assert_eq!(
        blend_marker(world, to_interpolated).total(),
        Duration::from_millis(500)
    );
}

#[test]
fn an_expired_window_clears_the_state_of_a_missing_component() {
    use bevy_ecs::system::RunSystemOnce;

    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(10.0), TestPos(10.0), Predicted))
        .id();
    app.world_mut().flush();
    send_switch(
        &mut app,
        entity,
        TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)),
    );
    app.world_mut().entity_mut(entity).remove::<TestPos>();
    app.world_mut().flush();

    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(600));
    app.world_mut()
        .run_system_once(CorrectionFn::<SimPos, TestPos, TestPos>::clear_expired_switch_state)
        .unwrap();
    app.world_mut()
        .run_system_once(TimelineSwitchPlugin::expire_switch_blends)
        .unwrap();
    app.world_mut().flush();

    let world = app.world();
    assert!(world.get::<SwitchBlend>(entity).is_none());
    assert!(
        world
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
    assert!(
        world
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
}

#[test]
fn a_switch_saves_no_visual_on_an_interpolated_entity() {
    fn histories(value: f32) -> (PredictionHistory<SimPos>, ConfirmedHistory<SimPos>) {
        let mut predicted = PredictionHistory::<SimPos>::default();
        predicted.add(NetTick(5), Some(SimPos(value)));
        let mut confirmed = ConfirmedHistory::<SimPos>::default();
        confirmed.insert(NetTick(5), HistoryState::Updated(SimPos(value)));
        (predicted, confirmed)
    }

    use crate::prediction_history::PredictionHistory;
    use crate::prediction_setup::PredictionSetup;
    use campfire_net_replication::ComponentRegistry;
    use campfire_net_timeline::Rollback;

    let mut app = switch_app();
    app.world_mut().init_resource::<ComponentRegistry>();
    app.world_mut().insert_resource(PredictionSetup::default());
    app.world_mut().insert_resource(Rollback(NetTick(5)));
    app.world_mut().resource_mut::<LocalTimeline>().shift(10);

    let (predicted_history, predicted_confirmed) = histories(5.0);
    let predicted = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Predicted,
            predicted_history,
            predicted_confirmed,
        ))
        .id();
    let (interp_history, interp_confirmed) = histories(5.0);
    let interp = app
        .world_mut()
        .spawn((
            SimPos(10.0),
            TestPos(10.0),
            Interpolated,
            interp_history,
            interp_confirmed,
        ))
        .id();
    app.world_mut().flush();
    prepare_rollback(&mut app);

    let world = app.world();
    // Both rewind to the rollback target ...
    assert_eq!(world.get::<SimPos>(predicted), Some(&SimPos(5.0)));
    assert_eq!(world.get::<SimPos>(interp), Some(&SimPos(5.0)));
    // ... but only the predicted one captures for correction.
    assert_eq!(
        world
            .get::<PreviousVisual<TestPos>>(predicted)
            .and_then(|slot| slot.0.as_ref())
            .cloned(),
        Some(TestPos(10.0))
    );
    assert!(
        world
            .get::<PreviousVisual<TestPos>>(interp)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
}

/// Which switches ask for a forced rollback. A switch to the prediction
/// timeline needs one: the world rewinds to the scanned frontier and replays
/// under the new marker, and that rollback is also the capture the blend
/// starts from. The other direction just presents the delayed value, and
/// before the first sync there is no scanned tick to rewind to, so the switch
/// applies without one.
#[test]
fn forced_rollback_is_requested_only_for_to_predicted_with_a_scanned_tick() {
    let k = NetTick(90);
    for (direction, scanned, expect_forced) in [
        (SwitchDirection::ToPredicted, Some(k), Some(k)),
        (SwitchDirection::ToInterpolated, Some(k), None),
        (SwitchDirection::ToPredicted, None, None),
    ] {
        let mut app = switch_app();
        app.init_resource::<RollbackCheckState>();
        if let Some(tick) = scanned {
            app.world_mut()
                .resource_mut::<RollbackCheckState>()
                .set_last_processed_confirmed_tick(tick);
        }
        let entity = match direction {
            SwitchDirection::ToPredicted => app
                .world_mut()
                .spawn((SimPos(10.0), TestPos(10.0), Interpolated))
                .id(),
            SwitchDirection::ToInterpolated => app
                .world_mut()
                .spawn((SimPos(10.0), TestPos(10.0), Predicted, FrameInterpolate))
                .id(),
        };
        app.world_mut().flush();

        send_switch(
            &mut app,
            entity,
            match direction {
                SwitchDirection::ToPredicted => {
                    TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500))
                }
                SwitchDirection::ToInterpolated => {
                    TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500))
                }
            },
        );

        let world = app.world();
        assert_eq!(
            world.get::<Predicted>(entity).is_some(),
            direction == SwitchDirection::ToPredicted,
            "{direction:?} must swap the markers"
        );
        assert!(world.get::<SwitchBlend>(entity).is_some());
        assert_eq!(
            world
                .resource::<RollbackCheckState>()
                .forced_rollback_tick(),
            expect_forced,
            "{direction:?} with scanned tick {scanned:?}"
        );
    }
}

/// A switch to the prediction timeline has nothing saved by the switch
/// itself: the rollback it asks for is what captures the value to blend
/// from. So when no rollback runs — before the first sync, where there is no
/// confirmed tick to rewind to — there is nothing to measure, and the switch
/// takes effect without a blend. Nothing is left behind either.
#[test]
fn to_predicted_without_a_rollback_has_nothing_to_blend_from() {
    let mut app = switch_app();
    app.init_resource::<RollbackCheckState>();
    let entity = app
        .world_mut()
        .spawn((SimPos(10.0), TestPos(10.0), Interpolated))
        .id();
    app.world_mut().flush();

    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    assert!(
        app.world()
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none(),
        "the switch saves nothing in this direction"
    );

    run_apply(&mut app);
    app.world_mut().get_mut::<TestPos>(entity).unwrap().0 = 30.0;
    run_blend(&mut app);

    let world = app.world();
    assert!(
        world
            .get::<PreviousVisual<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none(),
        "nothing was captured, because no rollback ran"
    );
    assert!(
        world
            .get::<VisualCorrection<TestPos>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none(),
        "no measurement means no correction: the switch is not blended"
    );
    // The window still runs its length, so the entity is not re-switched
    // while it is nominally blending.
    assert!(world.get::<SwitchBlend>(entity).is_some());
}

/// A committed switch holds the entity until it is applied: the save pass has
/// already recorded the value on screen for it, so a request arriving inside
/// that window must be dropped rather than resolved on top of it. Resolving
/// it would leave the recorded value to be measured as a jump that never
/// happened — the entity would be pinned back to the value from a frame the
/// replacement has nothing to do with — and would apply the replacement
/// against markers it never saw.
#[test]
fn requests_arriving_while_a_switch_is_committed_are_dropped() {
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(10.0), TestPos(10.0), Predicted))
        .id();
    app.world_mut().flush();

    // Frame F: the request is committed, and its value on screen recorded.
    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_interpolated().with_transition(Duration::from_millis(500)));
    run_save(&mut app);
    {
        let world = app.world();
        assert!(world.get::<PendingSwitch>(entity).is_some());
        assert!(world.get::<TimelineSwitch>(entity).is_none());
        assert!(
            world
                .get::<PreviousVisual<TestPos>>(entity)
                .and_then(|slot| slot.0.as_ref())
                .is_some()
        );
    }

    // A replacement arrives before the apply pass, with every field set so
    // its values cannot be told apart from a resolved request's. A second
    // save pass — which is what the window is at risk from, since the
    // recorded value is only meaningful for the switch it was taken for —
    // drops it and leaves the committed switch alone.
    app.world_mut().entity_mut(entity).insert(
        TimelineSwitch::to_interpolated()
            .with_transition(Duration::from_millis(250))
            .with_ease(CorrectionEase::Linear),
    );
    run_save(&mut app);
    {
        let world = app.world();
        assert!(
            world.get::<TimelineSwitch>(entity).is_none(),
            "a request that arrives while one is committed must not linger"
        );
        let committed = world
            .get::<PendingSwitch>(entity)
            .expect("committed switch");
        assert_eq!(
            committed.total,
            Duration::from_millis(500),
            "the committed switch is untouched"
        );
        assert_eq!(committed.ease, CorrectionEase::default());
    }

    // The committed switch is the one that applies, with the blend it was
    // committed with.
    run_apply(&mut app);
    let world = app.world();
    assert!(world.get::<Interpolated>(entity).is_some());
    assert!(world.get::<Predicted>(entity).is_none());
    assert!(world.get::<PendingSwitch>(entity).is_none());
    assert_eq!(
        blend_marker(world, entity).total(),
        Duration::from_millis(500)
    );

    // Nothing is left over to be applied later.
    run_save(&mut app);
    run_apply(&mut app);
    let world = app.world();
    assert!(world.get::<Interpolated>(entity).is_some());
    assert_eq!(
        blend_marker(world, entity).total(),
        Duration::from_millis(500)
    );
}

/// A request is a component, so two requests for the same entity in one
/// frame are two inserts: the second replaces the first. Only the request
/// that is left is served, and it is served against the timeline the entity
/// is actually on when the save pass runs.
#[test]
fn contradictory_requests_in_one_frame_keep_the_last_insert() {
    for (first, second, expect_predicted, expected_reason) in [
        // Up first, then down: the request left on the entity wants to
        // leave the prediction timeline, which an interpolated entity is
        // not on, so it is dropped and nothing changes.
        (
            SwitchDirection::ToPredicted,
            SwitchDirection::ToInterpolated,
            false,
            "the last insert is invalid on an interpolated entity",
        ),
        // Down first, then up: the request left is the valid one, so the
        // entity moves to the prediction timeline.
        (
            SwitchDirection::ToInterpolated,
            SwitchDirection::ToPredicted,
            true,
            "the last insert is valid on an interpolated entity",
        ),
    ] {
        let mut app = switch_app();
        let entity = app
            .world_mut()
            .spawn((SimPos(1.0), TestPos(1.0), Interpolated))
            .id();
        app.world_mut().flush();

        let request = |direction| match direction {
            SwitchDirection::ToPredicted => TimelineSwitch::to_predicted(),
            SwitchDirection::ToInterpolated => TimelineSwitch::to_interpolated(),
        };
        app.world_mut()
            .entity_mut(entity)
            .insert(request(first))
            .insert(request(second));
        run_save(&mut app);
        run_apply(&mut app);

        let world = app.world();
        assert_eq!(
            world.get::<Predicted>(entity).is_some(),
            expect_predicted,
            "{second:?} inserted after {first:?}: {expected_reason}"
        );
    }

    // Before the save pass commits, the last insert is the request that
    // sticks, including its blend length.
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(1.0), TestPos(1.0), Interpolated))
        .id();
    app.world_mut().flush();
    app.world_mut()
        .entity_mut(entity)
        .insert(TimelineSwitch::to_predicted().with_transition(Duration::from_millis(500)))
        .insert(TimelineSwitch::to_predicted().with_transition(Duration::from_millis(250)));
    run_save(&mut app);
    run_apply(&mut app);
    let world = app.world();
    assert!(world.get::<Predicted>(entity).is_some());
    assert_eq!(
        blend_marker(world, entity).total(),
        Duration::from_millis(250)
    );
}

#[test]
fn a_consumed_request_runs_once() {
    // One-shot semantics: running a handler with no request on the entity
    // changes nothing, even right after a switch.
    let mut app = switch_app();
    let entity = app
        .world_mut()
        .spawn((SimPos(1.0), TestPos(1.0), Interpolated))
        .id();
    app.world_mut().flush();

    send_switch(&mut app, entity, TimelineSwitch::to_predicted());
    assert!(app.world().get::<Predicted>(entity).is_some());
    // The request went with the markers it swapped.
    assert!(app.world().get::<TimelineSwitch>(entity).is_none());
    // No request left: both passes are noops.
    run_save(&mut app);
    run_apply(&mut app);
    assert!(app.world().get::<Predicted>(entity).is_some());
    assert!(app.world().get::<Interpolated>(entity).is_none());
}
