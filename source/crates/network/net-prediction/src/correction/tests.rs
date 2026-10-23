use crate::correction_ease::CorrectionEase;
use campfire_net_frame_interpolation::{
    AppFrameInterpolationExt, FrameInterpolate, FrameInterpolationHistory,
    FrameInterpolationPlugin, FrameRegistryPlugin, FrameRule, InterpolationSampleCx, linear_lerp,
};
use campfire_net_replication::internals::shared_replicon_app;

use bevy_curve::{Curve, Ease, FunctionCurve, Interval};
use bevy_ecs::system::RunSystemOnce;
use campfire_net_interpolation::InterpolationMarkerPlugin;
use campfire_net_replication::Diffable as ReplicationDiffable;
use campfire_net_replication::{AppComponentExt, Predicted};

use super::*;
use crate::prediction_marker_plugin::PredictionMarkerPlugin;
use crate::prediction_plugins::PredictionPlugins;
use crate::prediction_registration::prediction_frame_samples;
use crate::prediction_registration_ext::PredictionRegistrationExt;
use crate::prediction_registry_plugin::PredictionRegistryPlugin;
use bevy_ecs::world::FilteredEntityRef;
use bevy_time::Fixed;
use campfire_math::TickRate;
use campfire_net_core::NetTick;
use campfire_net_timeline::SetTickRate;
use core::time::Duration;

use crate::fixtures::prediction_stack::PredictionStack;
use crate::fixtures::protocol::{CompCorr, CompPredictionOnly, ShownCorr};
use crate::fixtures::steps::trigger_state_rollback;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use core::fmt::Debug;
use serde::{Deserialize, Serialize};

fn app_with_replication_markers() -> App {
    let mut app = shared_replicon_app();
    app.add_plugins((PredictionMarkerPlugin, InterpolationMarkerPlugin));
    // The correction systems stamp each error with the visual clock,
    // so a bounded curve can advance along its window.
    app.insert_resource(Time::<Virtual>::default());
    app
}

/// A predicted component, which shows through `ShownA`.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct CorrectionA(f32);

/// The visual of `CorrectionA`, which a correction offsets.
#[derive(Component, Clone, Debug, Default, PartialEq)]
struct ShownA(f32);

impl Ease for ShownA {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            Self(start.0 + (end.0 - start.0) * t)
        })
    }
}

impl ReplicationDiffable<Self> for ShownA {
    fn base_value() -> Self {
        Self::default()
    }

    fn diff(&self, new: &Self) -> Self {
        Self(new.0 - self.0)
    }

    fn apply_diff(&mut self, delta: &Self) {
        self.0 += delta.0;
    }
}

/// An entity whose archetype the frame rule of `CorrectionA` leaves out.
#[derive(Component, Debug)]
struct LeftOut;

fn show_a(start: &CorrectionA, end: &CorrectionA, cx: InterpolationSampleCx) -> ShownA {
    ShownA(start.0 + (end.0 - start.0) * cx.t)
}

/// An app that shows `CorrectionA` through `ShownA` on each entity but those with `LeftOut`, at an
/// overstep of 0, and corrects its rollbacks by `correct`; its plugins finished.
fn correction_app(correct: LerpFn<ShownA>) -> App {
    let mut app = app_with_replication_markers();
    app.init_resource::<PredictionRegistry>();
    app.insert_resource(PredictionSetup::default());
    app.insert_resource(Time::<Fixed>::default());
    app.add_plugins((FrameRegistryPlugin, FrameInterpolationPlugin));
    app.register_component::<CorrectionA>()
        .replicate()
        .predict()
        .correct_with::<ShownA, ShownA>(correct);
    app.register_frame_rule::<CorrectionA, ShownA, Without<LeftOut>>(FrameRule::new(show_a));
    app.finish();
    app
}

fn linear_app() -> App {
    correction_app(linear_lerp::<ShownA>)
}

/// Runs a frame's `PostUpdate`: the frame writes each visual from the live value, as the entities
/// have no frame history, and resolves the archetypes made since the last frame.
fn show(app: &mut App) {
    app.world_mut().run_schedule(PostUpdate);
}

fn error(app: &App, entity: Entity) -> Option<f32> {
    app.world()
        .get::<VisualCorrection<ShownA>>(entity)
        .and_then(|slot| slot.0.as_ref())
        .map(|correction| correction.error.0)
}

fn update(app: &mut App) {
    app.world_mut()
        .run_system_once(CorrectionFn::<CorrectionA, ShownA, ShownA>::update_visual_correction)
        .unwrap();
}

#[test]
fn prediction_adds_the_frame_interpolation_whose_visual_a_correction_offsets() {
    let mut server = app_with_replication_markers();
    server
        .register_component::<CorrectionA>()
        .replicate()
        .predict()
        .correct_linear::<ShownA, ShownA>();
    assert!(
        !server.is_plugin_added::<FrameInterpolationPlugin>(),
        "an app that does not predict runs no correction"
    );

    // The protocol registers before the plugin, which a protocol may.
    let mut app = app_with_replication_markers();
    app.register_component::<CorrectionA>()
        .replicate()
        .predict()
        .correct_linear::<ShownA, ShownA>();
    app.register_frame_rule::<CorrectionA, ShownA, ()>(FrameRule::new(show_a));
    app.add_plugins((
        FrameRegistryPlugin,
        PredictionRegistryPlugin,
        PredictionPlugins,
    ));

    assert!(app.is_plugin_added::<FrameInterpolationPlugin>());
    app.finish();

    let entity = app
        .world_mut()
        .spawn((CorrectionA(1.0), PreviousVisual(Some(ShownA(2.0)))))
        .id();
    app.world_mut().flush();
    assert!(app.world().get::<FrameInterpolate>(entity).is_some());
}

/// A converged offset is dropped whether or not a switch window is live.
/// Nothing re-derives an offset from the value a switch started from, so a
/// window does not need to keep the correction around: the next jump (a
/// rollback, or another switch) records a fresh one.
/// The frame a window opens moves the render. Counting the frame being
/// decayed is what makes that true: without it the first frame's keep ratio
/// is exactly `1`, which puts the rendered value back onto the one it already
/// had and shows the same pose twice.
#[test]
fn a_blend_first_frame_gives_up_part_of_the_gap() {
    for ease in [
        CorrectionEase::Linear,
        CorrectionEase::Smoothstep,
        CorrectionEase::EaseOutCubic,
        CorrectionEase::EaseInOutCubic,
    ] {
        let keep = ease.keep(
            Duration::ZERO,
            Duration::from_secs(1) / 60,
            Duration::from_secs(1),
        );
        assert!(
            keep < 1.0,
            "{ease:?} kept everything on the first frame: {keep}"
        );
        // A frame is the smallest step there is, so a curve should not give
        // up the whole window in one frame either.
        assert!(keep > 0.0, "{ease:?} gave up the whole gap at once: {keep}");
    }
}

/// Keeps half of the error whatever the ratio, so a test tells it from the linear decay.
fn halve_error(_converged: &ShownA, error: &ShownA, _ratio: f32) -> ShownA {
    ShownA(error.0 * 0.5)
}

/// `correct_with` decays the error by the registered function: an error of 4 keeps 2, and the
/// visual moves from the 10 the frame wrote to 10 + 2 = 12, while the component keeps 10. On an
/// entity whose archetype the frame rule leaves out, the frame writes no visual, −1 stays, and the
/// correction neither applies nor decays.
#[test]
fn correct_with_decays_the_error_by_its_function_where_the_frame_writes() {
    let mut app = correction_app(halve_error);
    let entity = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            VisualCorrection::active(ShownA(4.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    let left_out = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            ShownA(-1.0),
            LeftOut,
            VisualCorrection::active(ShownA(4.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    show(&mut app);
    assert_eq!(app.world().get::<ShownA>(entity), Some(&ShownA(10.0)));
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(16));

    update(&mut app);

    assert_eq!(error(&app, entity), Some(2.0));
    assert_eq!(app.world().get::<ShownA>(entity), Some(&ShownA(12.0)));
    assert_eq!(error(&app, left_out), Some(4.0));
    assert_eq!(app.world().get::<ShownA>(left_out), Some(&ShownA(-1.0)));
    for corrected in [entity, left_out] {
        assert_eq!(
            app.world().get::<CorrectionA>(corrected),
            Some(&CorrectionA(10.0))
        );
    }
}

/// A correction offsets a visual that each frame writes again, so one through a visual that no
/// frame rule shows the component through fails as the app finishes.
#[test]
#[should_panic(expected = "which no frame rule shows it through")]
fn a_correction_needs_a_frame_rule() {
    let mut app = app_with_replication_markers();
    app.register_component::<CorrectionA>()
        .replicate()
        .predict()
        .correct_linear::<ShownA, ShownA>();
    app.add_plugins((
        FrameRegistryPlugin,
        PredictionRegistryPlugin,
        PredictionPlugins,
    ));
    app.finish();
}

#[test]
#[should_panic(expected = "registers more than one correction")]
fn a_component_takes_one_correction() {
    let mut app = app_with_replication_markers();
    app.register_component::<CorrectionA>()
        .replicate()
        .predict()
        .correct_custom()
        .correct_linear::<ShownA, ShownA>();
}

#[test]
fn an_entity_correction_policy_overrides_the_global_one() {
    let mut app = linear_app();
    // Same unit error, but this entity decays on a 50 ms half schedule
    // instead of the global 200 ms one.
    let overridden = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            VisualCorrection::active(ShownA(1.0), Duration::ZERO),
            FrameInterpolate,
            CorrectionPolicy::new(0.5, Duration::from_millis(50)),
        ))
        .id();
    let plain = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            VisualCorrection::active(ShownA(1.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    show(&mut app);
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(125));

    update(&mut app);

    // Half of the error is left after each period: 0.5^(125 / 50) ≈ 0.177 of it on the override's,
    // and 0.5^(125 / 200) ≈ 0.648 on the default's, as the ease computes them.
    assert_eq!(error(&app, overridden), Some(0.5_f32.powf(0.125 / 0.05)));
    assert_eq!(error(&app, plain), Some(0.5_f32.powf(0.125 / 0.2)));
}

/// A saved value that appears after the system first ran moves its entity into an archetype that
/// did not exist then, and is still measured: 4 − 10 = −6.
#[test]
fn correction_measures_a_saved_value_in_an_archetype_created_later() {
    let mut app = linear_app();
    let create = |app: &mut App| {
        app.world_mut()
            .run_system_once(CorrectionFn::<CorrectionA, ShownA, ShownA>::create_visual_correction)
            .unwrap();
        app.world_mut().flush();
    };

    // First pass: nothing has a saved value yet.
    create(&mut app);

    // A saved value appears, which puts the entity in a new archetype.
    let entity = app
        .world_mut()
        .spawn((ShownA(10.0), PreviousVisual(Some(ShownA(4.0)))))
        .id();
    app.world_mut().flush();
    create(&mut app);

    assert_eq!(
        error(&app, entity),
        Some(-6.0),
        "the newly created archetype must be picked up"
    );
    assert!(
        app.world()
            .get::<PreviousVisual<ShownA>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
}

/// A bounded curve can be the correction policy too, not just a switch
/// window's curve. It is normalised over the policy's own duration, driven
/// by the clock the error was recorded at, and the error tracks the curve
/// exactly however the frames fall.
#[test]
fn correction_policy_can_run_a_bounded_curve() {
    let mut app = linear_app();
    // Half a second linear ramp.
    let duration = 0.5;
    let dt = 16.0 / 1000.0;
    // The app records a correction and applies it in the same frame, so its
    // start is stamped at the clock reading the first apply sees. Do the same
    // here, or the first frame would decay a window that has not started.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_secs_f32(dt));
    let start = app.world().resource::<Time<Virtual>>().elapsed();
    let policy =
        CorrectionPolicy::with_ease(CorrectionEase::Linear, Duration::from_secs_f32(duration));
    let entity = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            VisualCorrection::active(ShownA(1.0), start),
            FrameInterpolate,
            policy,
        ))
        .id();
    show(&mut app);

    // Step through the ramp: the error telescopes along the curve rather than
    // accumulating a per-frame factor, so it is always `remaining(elapsed +
    // dt)` — the frame being decayed counts, which is what stops a blend's
    // first frame from holding the render still.
    // The frame that records the correction applies it too, so decay one
    // frame before advancing the clock.
    let mut elapsed = 0.0f32;
    update(&mut app);
    assert_eq!(
        error(&app, entity),
        Some(1.0 - dt / duration),
        "the frame that records the correction already gives up one frame of it"
    );
    for _ in 0..20 {
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_secs_f32(dt));
        update(&mut app);
        let error = error(&app, entity);
        elapsed += dt;
        assert!(
            (error.unwrap() - (1.0 - (elapsed + dt) / duration)).abs() < 1e-6,
            "after {elapsed}s of a {duration}s linear ramp: {error:?}"
        );
    }

    // At the end of the ramp the error has converged exactly ...
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(400));
    update(&mut app);
    assert_eq!(
        error(&app, entity),
        None,
        "a bounded curve reaches zero at the end of its window, which ends the correction"
    );
    // ... and the following pass drops it, like any converged correction.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_millis(16));
    update(&mut app);
    assert_eq!(
        error(&app, entity),
        None,
        "a converged correction is dropped"
    );
}

#[test]
fn a_visual_correction_marks_its_visual_changed() {
    let mut app = linear_app();
    let entity = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            VisualCorrection::active(ShownA(1.0), Duration::ZERO),
            FrameInterpolate,
        ))
        .id();
    show(&mut app);
    app.world_mut().clear_trackers();
    let changed = |app: &App| {
        app.world()
            .entity(entity)
            .get_change_ticks::<ShownA>()
            .unwrap()
            .changed
    };
    let before = changed(&app);

    update(&mut app);

    assert_ne!(changed(&app), before);
}

// A capture without its live value (archetype mid-assembly: the value has
// not replicated yet) carries nothing correctable and must be skipped,
// not crash rule resolution.
#[test]
fn a_correction_after_a_rollback_measures_nothing_with_no_live_value() {
    let mut app = linear_app();
    app.world_mut().spawn((PreviousVisual(Some(ShownA(12.0))),));

    app.world_mut()
        .run_system_once(CorrectionFn::<CorrectionA, ShownA, ShownA>::create_visual_correction)
        .unwrap();
    let mut query = app.world_mut().query::<&VisualCorrection<ShownA>>();
    assert!(query.iter(app.world()).next().is_none());
}

/// A saved value with no live component to measure against is only kept
/// while a switch window is waiting for that component. Without one nothing
/// will ever use it, so it is discarded instead of lingering until despawn
/// and then being added to whatever the value becomes when it returns.
#[test]
fn creation_discards_a_saved_value_nobody_is_waiting_for() {
    let mut app = linear_app();

    // No live visual: the destination timeline has not presented it.
    let unwatched = app
        .world_mut()
        .spawn(PreviousVisual(Some(ShownA(8.0))))
        .id();
    // The same, but a switch window is waiting for the visual.
    let watched = app
        .world_mut()
        .spawn((
            PreviousVisual(Some(ShownA(8.0))),
            SwitchBlend::new(
                Duration::ZERO,
                Duration::from_millis(500),
                CorrectionEase::Linear,
            ),
        ))
        .id();
    app.world_mut().flush();

    app.world_mut()
        .run_system_once(CorrectionFn::<CorrectionA, ShownA, ShownA>::create_visual_correction)
        .unwrap();
    app.world_mut().flush();

    let world = app.world();
    assert!(
        world
            .get::<PreviousVisual<ShownA>>(unwatched)
            .and_then(|slot| slot.0.as_ref())
            .is_none(),
        "nothing was waiting for this value"
    );
    assert!(
        world
            .get::<PreviousVisual<ShownA>>(watched)
            .and_then(|slot| slot.0.as_ref())
            .is_some(),
        "the window must keep waiting for the component it will blend"
    );
}

fn replay_prediction_only(mut components: Query<'_, '_, &mut CompPredictionOnly, With<Predicted>>) {
    for mut component in &mut components {
        *component = CompPredictionOnly(10.0);
    }
}

fn replay_corrected(mut components: Query<'_, '_, &mut CompCorr, With<Predicted>>) {
    for mut component in &mut components {
        *component = CompCorr(10.0);
    }
}

fn history<C: Component + Clone>(tick: NetTick, value: C) -> PredictionHistory<C> {
    let mut history = PredictionHistory::default();
    history.add(tick, Some(value));
    history
}

fn set_correction_sampling_time(stepper: &mut Stepper<PredictionStack>) {
    let world = stepper.client_world_mut(0);
    world.trigger(SetTickRate(TickRate::try_from(1).unwrap()));
    world
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(500));
}

/// A policy that gives up nothing, so a correction can be observed exactly as it
/// was recorded instead of part-way through its decay.
fn freeze_correction(stepper: &mut Stepper<PredictionStack>, entity: Entity) {
    stepper
        .client_world_mut(0)
        .entity_mut(entity)
        .insert(CorrectionPolicy::new(1.0, Duration::from_secs(3600)));
}

/// Runs the rest of the frame after `PreUpdate`: frame interpolation writes the
/// value this frame renders, the creation system records the correction from it,
/// and the apply adds it.
fn finish_frame_with_correction(stepper: &mut Stepper<PredictionStack>) {
    stepper.client_world_mut(0).run_schedule(PostUpdate);
}

/// After a rollback, frame interpolation reads a predicted component from the prediction history
/// the replay rewrote, its frame source: 4 at the tick before and 10 at the tick, whatever a frame
/// history on the entity still holds.
#[test]
fn frame_interpolation_reads_the_replayed_prediction_history() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, replay_prediction_only);

    let current_tick = stepper.client_tick(0);
    let rollback_tick = current_tick - 1;
    let entity = stepper
        .client_world_mut(0)
        .spawn((
            Predicted,
            CompPredictionOnly(100.0),
            history(rollback_tick, CompPredictionOnly(4.0)),
            FrameInterpolationHistory::<CompPredictionOnly> {
                previous_value: Some(CompPredictionOnly(200.0)),
                current_value: Some(CompPredictionOnly(300.0)),
            },
        ))
        .id();

    trigger_state_rollback(&mut stepper, rollback_tick);
    stepper.client_world_mut(0).run_schedule(PreUpdate);

    let world = stepper.client_world_mut(0);
    let mut sources = QueryBuilder::<FilteredEntityRef<'_, '_>>::new(world)
        .data::<&PredictionHistory<CompPredictionOnly>>()
        .build();
    let source = sources.get(world, entity).unwrap();
    let samples = prediction_frame_samples::<CompPredictionOnly>(&source, current_tick);
    assert_eq!(samples.previous, Some(&CompPredictionOnly(4.0)));
    assert_eq!(samples.current, Some(&CompPredictionOnly(10.0)));
}

/// A rollback records the jump between the visual on screen and the visual the frame writes after
/// the replay, and holds the render there while the component takes the replayed value. Before
/// it, the prediction history holds 0 at the tick before and, at or before the tick, 0 again, so
/// the frame shows 0 though the component holds 1. The replay writes 10 at the tick, which the
/// frame shows half way, 0 + (10 − 0) · 0.5 = 5; the error is 0 − 5 = −5, and the visual
/// 5 − 5 = 0, while the component holds 10.
#[test]
fn a_rollback_holds_the_visual_where_it_was_and_the_component_takes_the_replay() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    set_correction_sampling_time(&mut stepper);
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, replay_corrected);

    let current_tick = stepper.client_tick(0);
    let rollback_tick = current_tick - 1;
    let entity = stepper
        .client_world_mut(0)
        .spawn((
            Predicted,
            CompCorr(1.0),
            history(rollback_tick, CompCorr(0.0)),
        ))
        .id();
    freeze_correction(&mut stepper, entity);
    finish_frame_with_correction(&mut stepper);
    // The history's slots went on with it, so the rollback and the corrections it records
    // write them in place and move the entity to no other table.
    let table = stepper.client_world(0).entity(entity).archetype().id();
    assert_eq!(
        stepper.client_world(0).get::<ShownCorr>(entity),
        Some(&ShownCorr(0.0))
    );

    trigger_state_rollback(&mut stepper, rollback_tick);
    stepper.client_world_mut(0).run_schedule(PreUpdate);
    {
        let world = stepper.client_world(0);
        assert_eq!(world.resource::<Time<Fixed>>().overstep_fraction(), 0.5);
        assert_eq!(world.get::<CompCorr>(entity), Some(&CompCorr(10.0)));
        assert_eq!(
            world
                .get::<PreviousVisual<ShownCorr>>(entity)
                .and_then(|slot| slot.0.clone()),
            Some(ShownCorr(0.0))
        );
    }

    finish_frame_with_correction(&mut stepper);

    let world = stepper.client_world(0);
    assert_eq!(world.entity(entity).archetype().id(), table);
    assert_eq!(
        world
            .get::<VisualCorrection<ShownCorr>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .map(|correction| correction.error.clone()),
        Some(ShownCorr(-5.0))
    );
    assert_eq!(world.get::<ShownCorr>(entity), Some(&ShownCorr(0.0)));
    assert_eq!(world.get::<CompCorr>(entity), Some(&CompCorr(10.0)));
    // The saved value was spent recording the correction.
    assert!(
        world
            .get::<PreviousVisual<ShownCorr>>(entity)
            .and_then(|slot| slot.0.as_ref())
            .is_none()
    );
}

/// The default exponential runs until it keeps under a thousandth of its error: half every
/// 200 ms takes 10 periods, 2 s, as two rounded logarithms give it.
#[test]
fn the_default_correction_window_is_its_convergence() {
    let window = CorrectionEase::exponential_window(0.5, Duration::from_millis(200), CONVERGED);
    assert!((window.as_secs_f32() - 2.0).abs() < 1e-5, "{window:?}");
}

/// An exponential correction ends at its window's end, before f32 decay could stall it: at
/// 30 ms frames, the 67th frame reaches 2.01 s past the 2 s window, while the product of the
/// 66 before it still keeps 0.5^9.9, about a 955th of the jump.
#[test]
fn an_exponential_correction_ends_at_its_window() {
    let mut app = linear_app();
    let dt = Duration::from_millis(30);
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(dt);
    let start = app.world().resource::<Time<Virtual>>().elapsed();
    let entity = app
        .world_mut()
        .spawn((
            CorrectionA(10.0),
            VisualCorrection::active(ShownA(1.0), start),
            FrameInterpolate,
        ))
        .id();
    show(&mut app);

    for frame in 0..67 {
        assert!(error(&app, entity).is_some(), "frame {frame}");
        update(&mut app);
        app.world_mut().flush();
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(dt);
    }
    assert_eq!(error(&app, entity), None);
}

/// The systems that read and write one correction's state run in a fixed order: none of them is
/// ambiguous with another on `PostUpdate`.
#[test]
fn the_correction_systems_have_one_order() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    stepper.frame_step(1);
    let app = stepper.client_app(0);
    let schedules = app.world().resource::<Schedules>();
    let schedule = schedules.get(PostUpdate).unwrap();
    let names = [
        "CorrectionFn::create_visual_correction<",
        "CorrectionFn::update_visual_correction<",
        "CorrectionFn::clear_expired_switch_state<",
        "CorrectionFn::save_switch_visual<",
        "TimelineSwitchPlugin::expire_switch_blends",
        "TimelineSwitchPlugin::save_previous_visuals",
    ];
    let name = |key| {
        schedule
            .systems()
            .unwrap()
            .find(|(system, _)| *system == key)
            .map(|(_, system)| system.name().to_string())
            .unwrap()
    };
    let ours = |key| {
        let name = name(key);
        names.iter().any(|prefix| name.contains(prefix))
    };
    let ambiguous: Vec<_> = schedule
        .graph()
        .conflicting_systems()
        .iter()
        .filter(|(a, b, _)| ours(*a) && ours(*b))
        .map(|(a, b, _)| (name(*a), name(*b)))
        .collect();
    assert!(ambiguous.is_empty(), "{ambiguous:#?}");
}
