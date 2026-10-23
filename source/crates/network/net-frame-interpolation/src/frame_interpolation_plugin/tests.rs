use super::*;
use crate::app_frame_interpolation_ext::AppFrameInterpolationExt;
use crate::frame_interpolate::FrameInterpolate;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::frame_registry::FrameRegistry;
use crate::frame_registry_plugin::FrameRegistryPlugin;
use crate::frame_rule::FrameRule;
use crate::frame_source::{FrameSamples, FrameSource};
use crate::interpolation_sample_cx::InterpolationSampleCx;
use crate::skip_frame_interpolation::SkipFrameInterpolation;
use bevy_ecs::world::FilteredEntityRef;
use bevy_time::{Fixed, Time};
use campfire_net_core::NetTick;
use campfire_net_timeline::{LocalTimeline, Rollback};
use core::time::Duration;

#[derive(Component, Clone, Debug, PartialEq)]
struct FrameA(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct ShownA(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct FrameB(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct ShownB(f32);

#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
struct SparseFrame(f32);

#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
struct SparseShown(f32);

#[derive(Debug, Component)]
struct Opted;

/// The blend of `FrameA` into `ShownA`, offset by `offset`, so a test tells which rule wrote.
fn blend_a<const OFFSET: i32>(start: &FrameA, end: &FrameA, cx: InterpolationSampleCx) -> ShownA {
    ShownA(OFFSET as f32 + start.0 + (end.0 - start.0) * cx.t)
}

fn blend_b(start: &FrameB, end: &FrameB, cx: InterpolationSampleCx) -> ShownB {
    ShownB(start.0 + (end.0 - start.0) * cx.t)
}

fn blend_sparse(start: &SparseFrame, end: &SparseFrame, cx: InterpolationSampleCx) -> SparseShown {
    SparseShown(start.0 + (end.0 - start.0) * cx.t)
}

/// The fixed timestep and the overstep a frame blends at.
fn app_at(timestep: Duration, overstep: Duration) -> App {
    let mut app = App::new();
    let mut fixed = Time::<Fixed>::from_duration(timestep);
    fixed.accumulate_overstep(overstep);
    app.insert_resource(fixed);
    app.add_plugins((FrameRegistryPlugin, FrameInterpolationPlugin));
    app
}

/// A quarter of a 1 s tick.
fn quarter_app() -> App {
    app_at(Duration::from_secs(1), Duration::from_millis(250))
}

fn history_a(previous: f32, current: f32) -> FrameInterpolationHistory<FrameA> {
    FrameInterpolationHistory {
        previous_value: Some(FrameA(previous)),
        current_value: Some(FrameA(current)),
    }
}

#[test]
#[should_panic(expected = "the frame registry takes a registration after its plugin finished")]
fn the_frame_registry_refuses_a_rule_registered_late() {
    let mut app = quarter_app();
    app.finish();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::disabled());
}

#[test]
#[should_panic(expected = "is shown through itself")]
fn a_component_is_not_its_own_visual() {
    quarter_app().register_frame_rule::<FrameA, FrameA, ()>(FrameRule::disabled());
}

/// Adding `FrameInterpolate`, or the component, later, makes the frame history; spawned together,
/// so do they, and a replay's ticks record too.
#[test]
fn the_frame_history_comes_with_the_marker_and_the_component_and_records_a_replay() {
    let mut app = quarter_app();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(blend_a::<0>));
    app.finish();
    let history = |app: &App, entity| {
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_some()
    };

    let marked_later = app.world_mut().spawn(FrameA(1.0)).id();
    let component_later = app.world_mut().spawn(FrameInterpolate).id();
    let both = app.world_mut().spawn((FrameA(1.0), FrameInterpolate)).id();
    app.world_mut().run_schedule(FixedPostUpdate);
    assert!(!history(&app, marked_later));
    assert!(!history(&app, component_later));
    assert!(history(&app, both));

    app.world_mut()
        .entity_mut(marked_later)
        .insert(FrameInterpolate);
    app.world_mut()
        .entity_mut(component_later)
        .insert(FrameA(1.0));
    app.world_mut().run_schedule(FixedPostUpdate);
    assert!(history(&app, marked_later));
    assert!(history(&app, component_later));

    app.insert_resource(Rollback(NetTick(0)));
    for value in [2.0, 3.0] {
        app.world_mut().get_mut::<FrameA>(both).unwrap().0 = value;
        app.world_mut().run_schedule(FixedPostUpdate);
    }
    app.world_mut().remove_resource::<Rollback>();
    assert_eq!(
        app.world().get::<FrameInterpolationHistory<FrameA>>(both),
        Some(&history_a(2.0, 3.0))
    );
}

/// A rule writes its visual from the frame history at the overstep, 10 + 0 + (8 − 0) · 0.25 = 12,
/// and leaves the component, and its change tick, as they were; a second rule of another
/// component writes its own visual on the same entity.
#[test]
fn a_frame_rule_writes_the_visual_and_leaves_the_component() {
    let mut app = quarter_app();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(blend_a::<10>));
    app.register_frame_rule::<FrameB, ShownB, ()>(FrameRule::new(blend_b));
    app.finish();
    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            history_a(0.0, 8.0),
            FrameB(-2.0),
            FrameInterpolationHistory::<FrameB> {
                previous_value: Some(FrameB(4.0)),
                current_value: Some(FrameB(8.0)),
            },
        ))
        .id();
    app.world_mut().clear_trackers();
    let changed = |app: &App| {
        app.world()
            .entity(entity)
            .get_change_ticks::<FrameA>()
            .unwrap()
            .changed
    };
    let before = changed(&app);

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<ShownA>(entity), Some(&ShownA(12.0)));
    // 4 + (8 − 4) · 0.25 = 5.
    assert_eq!(app.world().get::<ShownB>(entity), Some(&ShownB(5.0)));
    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(-1.0)));
    assert_eq!(changed(&app), before);
}

/// A blend reads the fixed timestep as its sample's interval, 2 s, and the overstep, 0.25.
#[test]
fn a_frame_rule_reads_the_fixed_timestep() {
    let mut app = app_at(Duration::from_secs(2), Duration::from_millis(500));
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(|_, _, cx| {
        ShownA(cx.t + 10.0 * cx.sample_delta_secs.unwrap_or_default())
    }));
    app.finish();
    let entity = app
        .world_mut()
        .spawn((FrameInterpolate, FrameA(0.0), history_a(0.0, 10.0)))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<ShownA>(entity), Some(&ShownA(20.25)));
}

/// An entity that is not frame-interpolated shows its live value, 3 + (3 − 3) · 0.25 = 3; one with
/// no previous sample its newest, 8; one that skips frame interpolation its newest too, 8, and
/// its frame history then holds it as its previous value.
#[test]
fn a_visual_with_no_blend_shows_the_newest_value() {
    let mut app = quarter_app();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(blend_a::<0>));
    app.finish();
    let live = app.world_mut().spawn(FrameA(3.0)).id();
    let fresh = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(8.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: None,
                current_value: Some(FrameA(8.0)),
            },
        ))
        .id();
    let skips = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            SkipFrameInterpolation,
            FrameA(8.0),
            history_a(0.0, 8.0),
        ))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    let shown = |entity| app.world().get::<ShownA>(entity).cloned();
    assert_eq!(shown(live), Some(ShownA(3.0)));
    assert_eq!(shown(fresh), Some(ShownA(8.0)));
    assert_eq!(shown(skips), Some(ShownA(8.0)));
    assert_eq!(
        app.world().get::<FrameInterpolationHistory<FrameA>>(skips),
        Some(&history_a(8.0, 8.0))
    );
}

/// Of the rules that match, the highest priority writes: the filtered one, 20 + 4 + 8 · 0.25 = 26,
/// on the entity its marker opts in; the broad one, 10 + 4 + 2 = 16, elsewhere. A disabled rule
/// that wins writes nothing, and keeps no frame history.
#[test]
fn the_rule_of_the_highest_priority_writes_and_a_disabled_one_writes_nothing() {
    let mut app = quarter_app();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(blend_a::<10>));
    app.register_frame_rule::<FrameA, ShownA, With<Opted>>(
        FrameRule::new(blend_a::<20>).priority(100),
    );
    app.register_frame_rule::<FrameB, ShownB, ()>(FrameRule::new(blend_b));
    app.register_frame_rule::<FrameB, ShownB, With<Opted>>(FrameRule::disabled().priority(100));
    app.finish();
    let disabled = app
        .world_mut()
        .spawn((FrameInterpolate, Opted, FrameB(1.0)))
        .id();
    app.world_mut().run_schedule(FixedPostUpdate);
    let opted = app
        .world_mut()
        .spawn((FrameInterpolate, Opted, FrameA(-1.0), history_a(4.0, 12.0)))
        .id();
    let broad = app
        .world_mut()
        .spawn((FrameInterpolate, FrameA(-1.0), history_a(4.0, 12.0)))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<ShownA>(opted), Some(&ShownA(26.0)));
    assert_eq!(app.world().get::<ShownA>(broad), Some(&ShownA(16.0)));
    assert_eq!(app.world().get::<ShownB>(disabled), None);
    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameB>>(disabled)
            .is_none()
    );
}

/// A sparse-set component's history records each tick, 1 then 3; its sparse-set visual blends
/// them, 1 + (3 − 1) · 0.5 = 2, and the component keeps 3 through the fixed loop that follows.
#[test]
fn a_sparse_set_component_shows_through_a_sparse_set_visual() {
    let mut app = app_at(Duration::from_secs(1), Duration::from_millis(500));
    app.register_frame_rule::<SparseFrame, SparseShown, ()>(FrameRule::new(blend_sparse));
    app.finish();
    let entity = app
        .world_mut()
        .spawn((FrameInterpolate, SparseFrame(1.0)))
        .id();

    app.world_mut().run_schedule(FixedPostUpdate);
    app.world_mut().entity_mut(entity).insert(SparseFrame(3.0));
    app.world_mut().run_schedule(FixedPostUpdate);
    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(
        app.world().get::<SparseShown>(entity),
        Some(&SparseShown(2.0))
    );
    app.world_mut().run_schedule(RunFixedMainLoop);
    assert_eq!(
        app.world().get::<SparseFrame>(entity),
        Some(&SparseFrame(3.0))
    );
}

#[derive(Debug, Resource, Default, Clone, Copy, PartialEq, Eq)]
struct ObservedChanges {
    table: bool,
    sparse_set: bool,
}

fn observe_changes(
    table: Query<'_, '_, (), Changed<ShownA>>,
    sparse_set: Query<'_, '_, (), Changed<SparseShown>>,
    mut observed: ResMut<'_, ObservedChanges>,
) {
    observed.table = !table.is_empty();
    observed.sparse_set = !sparse_set.is_empty();
}

/// The same samples again write nothing, so a stationary entity's visual is not `Changed`; new
/// samples change a table and a sparse-set visual alike.
#[test]
fn a_visual_changes_only_when_its_value_does() {
    let mut app = app_at(Duration::from_secs(1), Duration::ZERO);
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(|_, end, _| ShownA(end.0)));
    app.register_frame_rule::<SparseFrame, SparseShown, ()>(FrameRule::new(|_, end, _| {
        SparseShown(end.0)
    }));
    app.init_resource::<ObservedChanges>();
    app.add_systems(
        PostUpdate,
        observe_changes.after(FrameInterpolationSystems::Interpolate),
    );
    app.finish();
    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(0.0),
            SparseFrame(0.0),
            history_a(0.0, 1.0),
            FrameInterpolationHistory::<SparseFrame> {
                previous_value: Some(SparseFrame(0.0)),
                current_value: Some(SparseFrame(1.0)),
            },
        ))
        .id();
    let run_and_observe = |app: &mut App| {
        *app.world_mut().resource_mut::<ObservedChanges>() = ObservedChanges::default();
        app.world_mut().run_schedule(PostUpdate);
        *app.world().resource::<ObservedChanges>()
    };

    // The first run inserts the visuals; the second sees what the insert left.
    run_and_observe(&mut app);
    run_and_observe(&mut app);
    assert_eq!(run_and_observe(&mut app), ObservedChanges::default());

    let mut entity_mut = app.world_mut().entity_mut(entity);
    entity_mut
        .get_mut::<FrameInterpolationHistory<FrameA>>()
        .unwrap()
        .current_value = Some(FrameA(2.0));
    entity_mut
        .get_mut::<FrameInterpolationHistory<SparseFrame>>()
        .unwrap()
        .current_value = Some(SparseFrame(2.0));
    assert_eq!(
        run_and_observe(&mut app),
        ObservedChanges {
            table: true,
            sparse_set: true,
        }
    );
}

/// A source the test keeps: `FrameA` at tick 6 and the tick before.
#[derive(Debug, Component)]
struct FrameASamples {
    previous: FrameA,
    current: FrameA,
}

/// The samples of `FrameA` that the source holds: none but at tick 6.
fn frame_a_samples<'w>(
    source: &FilteredEntityRef<'w, '_>,
    tick: NetTick,
) -> FrameSamples<'w, FrameA> {
    let Some(samples) = source.get::<FrameASamples>() else {
        return FrameSamples::NONE;
    };
    if tick == NetTick(6) {
        FrameSamples {
            previous: Some(&samples.previous),
            current: Some(&samples.current),
        }
    } else {
        FrameSamples::NONE
    }
}

/// An archetype with a frame source reads its samples through it at the app's tick: it gets no
/// frame history, and the visual at an overstep of 0.25 is 2 + (6 − 2) · 0.25 = 3. At tick 7 the
/// source holds nothing, and the visual shows the live value, 9.
#[test]
fn a_frame_source_supplies_the_samples() {
    let mut app = quarter_app();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(blend_a::<0>));
    let source_id = app.world_mut().register_component::<FrameASamples>();
    app.world_mut()
        .resource_mut::<FrameRegistry>()
        .set_frame_source::<FrameA>(FrameSource::new::<FrameA>(source_id, frame_a_samples));
    app.finish();
    let mut timeline = LocalTimeline::default();
    timeline.shift(6);
    app.insert_resource(timeline);
    let entity = app
        .world_mut()
        .spawn((
            FrameA(0.0),
            FrameInterpolate,
            FrameASamples {
                previous: FrameA(2.0),
                current: FrameA(6.0),
            },
        ))
        .id();
    let shown = |app: &App| app.world().get::<ShownA>(entity).cloned();

    app.world_mut().run_schedule(FixedPostUpdate);
    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_none()
    );
    app.world_mut().run_schedule(PostUpdate);
    assert_eq!(shown(&app), Some(ShownA(3.0)));

    app.world_mut().resource_mut::<LocalTimeline>().shift(1);
    app.world_mut().entity_mut(entity).insert(FrameA(9.0));
    app.world_mut().run_schedule(PostUpdate);
    assert_eq!(shown(&app), Some(ShownA(9.0)));
}

/// Adds 1 to each `FrameA` in each fixed tick, as a sim does.
fn step_frame_a(mut query: Query<'_, '_, &mut FrameA>) {
    for mut value in &mut query {
        value.0 += 1.0;
    }
}

/// A fixed-tick component with no frame source equals its frame history's newest value at each
/// tick while its visual blends the last two, 0.25 of the way; a rollback's replay, which runs the
/// fixed ticks again after a frame wrote the visual, steps from the simulated value, never from the
/// visual one.
#[test]
fn the_component_keeps_its_simulated_value_through_a_frame_and_a_replay() {
    let mut app = quarter_app();
    app.register_frame_rule::<FrameA, ShownA, ()>(FrameRule::new(blend_a::<0>));
    app.add_systems(FixedUpdate, step_frame_a);
    app.finish();
    let entity = app.world_mut().spawn((FrameInterpolate, FrameA(0.0))).id();
    let tick = |app: &mut App| {
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(FixedPostUpdate);
    };

    for simulated in [1.0, 2.0, 3.0] {
        tick(&mut app);
        app.world_mut().run_schedule(PostUpdate);
        let world = app.world();
        let history = world.get::<FrameInterpolationHistory<FrameA>>(entity);
        assert_eq!(world.get::<FrameA>(entity), Some(&FrameA(simulated)));
        assert_eq!(
            history.and_then(|history| history.current_value.clone()),
            Some(FrameA(simulated))
        );
    }
    // After ticks 2 and 3: 2 + (3 − 2) · 0.25.
    assert_eq!(app.world().get::<ShownA>(entity), Some(&ShownA(2.25)));

    app.insert_resource(Rollback(NetTick(0)));
    tick(&mut app);
    app.world_mut().remove_resource::<Rollback>();
    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(4.0)));
}
