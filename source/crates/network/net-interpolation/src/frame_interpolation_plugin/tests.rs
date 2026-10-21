use super::*;
use campfire_net_replication::internals::shared_replicon_app;

use crate::app_interpolation_ext::AppInterpolationExt;

use crate::interpolation_fns::InterpolationFns;
use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
use crate::interpolation_rule::InterpolationRule;

use crate::frame_interpolate::FrameInterpolate;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use campfire_net_replication::ConfirmedHistory;
use core::time::Duration;

use campfire_net_replication::AppComponentExt;

use serde::{Deserialize, Serialize};

use crate::frame_source::{FrameSamples, FrameSource};
use crate::interpolation_sample_cx::InterpolationSampleCx;
use bevy_ecs::world::FilteredEntityRef;
use bevy_time::{Fixed, Time};
use campfire_net_core::NetTick;
use campfire_net_timeline::LocalTimeline;

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
struct FrameA(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct FrameB(f32);

#[derive(Debug, Component)]
struct DisabledFrameHistory;

#[derive(Component, Clone, Debug, PartialEq)]
#[component(storage = "SparseSet")]
struct SparseFrame(f32);

#[derive(Debug, Resource, Default, Clone, Copy, PartialEq, Eq)]
struct ObservedFrameInterpolationChanges {
    table: bool,
    sparse_set: bool,
}

fn observe_frame_interpolation_changes(
    table: Query<'_, '_, (), Changed<FrameA>>,
    sparse_set: Query<'_, '_, (), Changed<SparseFrame>>,
    mut observed: ResMut<'_, ObservedFrameInterpolationChanges>,
) {
    observed.table = !table.is_empty();
    observed.sparse_set = !sparse_set.is_empty();
}

fn bundle_lerp(start: (&FrameA, &FrameB), end: (&FrameA, &FrameB), t: f32) -> (FrameA, FrameB) {
    (
        FrameA(100.0 + start.0.0 + (end.0.0 - start.0.0) * t),
        FrameB(200.0 + start.1.0 + (end.1.0 - start.1.0) * t),
    )
}

fn bundle_cx_lerp(
    _start: (&FrameA, &FrameB),
    _end: (&FrameA, &FrameB),
    cx: InterpolationSampleCx,
) -> (FrameA, FrameB) {
    (
        FrameA(cx.t),
        FrameB(cx.sample_delta_secs.unwrap_or_default()),
    )
}

/// Rule registration is rejected after the frame plugin finalizes the registry.
#[test]
#[should_panic(
    expected = "cannot register interpolation rules after InterpolationRegistry has been finalized"
)]
fn the_frame_plugin_refuses_a_rule_registered_late() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.finish();

    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::disabled(),
    ));
}

/// Adding `FrameInterpolate` later initializes existing component history.
#[test]
fn a_later_frame_interpolate_inserts_the_frame_history() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    let entity = app.world_mut().spawn(FrameA(1.0)).id();
    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_none()
    );

    app.world_mut().entity_mut(entity).insert(FrameInterpolate);
    app.world_mut().run_schedule(FixedPostUpdate);

    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_some()
    );
}

/// Adding a registered component later initializes its frame history.
#[test]
fn a_later_component_inserts_its_frame_history() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    let entity = app.world_mut().spawn(FrameInterpolate).id();
    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_none()
    );

    app.world_mut().entity_mut(entity).insert(FrameA(1.0));
    app.world_mut().run_schedule(FixedPostUpdate);

    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_some()
    );
}

/// Spawning the marker and component together initializes frame history.
#[test]
fn frame_update_inserts_history_when_both_are_spawned() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    let entity = app.world_mut().spawn((FrameA(1.0), FrameInterpolate)).id();
    app.world_mut().run_schedule(FixedPostUpdate);

    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_some()
    );
}

/// A winning disabled rule blocks lower-priority frame-history creation.
#[test]
fn a_winning_disabled_rule_inserts_no_frame_history_of_a_lower_rule() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.register_interpolation_rule::<FrameA, With<DisabledFrameHistory>>(
        InterpolationRule::new(InterpolationFns::disabled()).priority(100),
    );
    app.finish();

    let entity = app
        .world_mut()
        .spawn((FrameA(1.0), DisabledFrameHistory, FrameInterpolate))
        .id();
    app.world_mut().run_schedule(FixedPostUpdate);

    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_none()
    );
}

/// Every component a winning rule holds gets its frame history.
#[test]
fn frame_update_initializes_all_histories_for_the_winning_rules() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.register_interpolation_rule::<FrameB, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameB, end: &FrameB, t: f32| {
            FrameB(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    let entity = app
        .world_mut()
        .spawn((FrameA(1.0), FrameB(2.0), FrameInterpolate))
        .id();
    app.world_mut().run_schedule(FixedPostUpdate);

    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_some()
    );
    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameB>>(entity)
            .is_some()
    );
}

#[test]
fn a_frame_bundle_interpolates_by_its_tuple_rule() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(500));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_bundle_interpolation_rule::<(FrameA, FrameB), ()>(InterpolationRule::new(
        InterpolationFns::no_history(bundle_lerp),
    ));
    app.finish();

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            FrameB(-1.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(0.0)),
                current_value: Some(FrameA(10.0)),
            },
            FrameInterpolationHistory::<FrameB> {
                previous_value: Some(FrameB(0.0)),
                current_value: Some(FrameB(20.0)),
            },
        ))
        .id();
    app.world_mut().clear_trackers();
    let first_changed = app
        .world()
        .entity(entity)
        .get_change_ticks::<FrameA>()
        .unwrap()
        .changed;
    let second_changed = app
        .world()
        .entity(entity)
        .get_change_ticks::<FrameB>()
        .unwrap()
        .changed;

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(105.0)));
    assert_eq!(app.world().get::<FrameB>(entity), Some(&FrameB(210.0)));
    assert_ne!(
        app.world()
            .entity(entity)
            .get_change_ticks::<FrameA>()
            .unwrap()
            .changed,
        first_changed
    );
    assert_ne!(
        app.world()
            .entity(entity)
            .get_change_ticks::<FrameB>()
            .unwrap()
            .changed,
        second_changed
    );
}

/// A bundle can hold the histories while a component rule applies the live member.
#[test]
fn a_frame_bundle_keeps_the_histories_while_a_component_rule_applies_the_live_member() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(500));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.register_bundle_interpolation_rule::<(FrameA, FrameB), ()>(InterpolationRule::new(
        InterpolationFns::no_history(bundle_lerp),
    ));
    app.finish();

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(0.0)),
                current_value: Some(FrameA(10.0)),
            },
            FrameInterpolationHistory::<FrameB> {
                previous_value: Some(FrameB(0.0)),
                current_value: Some(FrameB(20.0)),
            },
        ))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(5.0)));
    assert!(!app.world().entity(entity).contains::<FrameB>());
}

#[test]
fn a_frame_bundle_interpolation_gets_the_fixed_timestep() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(2)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(500));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_bundle_interpolation_rule::<(FrameA, FrameB), ()>(InterpolationRule::new(
        InterpolationFns::no_history_with_cx(bundle_cx_lerp),
    ));
    app.finish();

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            FrameB(-1.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(0.0)),
                current_value: Some(FrameA(10.0)),
            },
            FrameInterpolationHistory::<FrameB> {
                previous_value: Some(FrameB(0.0)),
                current_value: Some(FrameB(20.0)),
            },
        ))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(0.25)));
    assert_eq!(app.world().get::<FrameB>(entity), Some(&FrameB(2.0)));
}

#[test]
fn frame_interpolation_reuses_no_history_interpolation_rule() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(250));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(10.0 + start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    assert!(
        app.world()
            .components()
            .component_id::<ConfirmedHistory<FrameA>>()
            .is_some()
    );

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(0.0)),
                current_value: Some(FrameA(8.0)),
            },
        ))
        .id();
    app.world_mut().clear_trackers();
    let changed = app
        .world()
        .entity(entity)
        .get_change_ticks::<FrameA>()
        .unwrap()
        .changed;

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(12.0)));
    assert_ne!(
        app.world()
            .entity(entity)
            .get_change_ticks::<FrameA>()
            .unwrap()
            .changed,
        changed
    );
}

#[test]
fn a_frame_interpolation_takes_a_history_only_rule() {
    let mut app = shared_replicon_app();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(250));
    app.add_plugins(InterpolationMarkerPlugin);
    app.register_component::<FrameA>().replicate();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::history_only().interpolate(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(20.0 + start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    assert!(
        app.world()
            .components()
            .component_id::<ConfirmedHistory<FrameA>>()
            .is_some()
    );

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(4.0)),
                current_value: Some(FrameA(12.0)),
            },
        ))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(26.0)));
}

#[test]
fn a_frame_interpolation_takes_the_rule_filtered_by_its_marker() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(250));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(10.0 + start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.register_interpolation_rule::<FrameA, With<FrameInterpolate>>(
        InterpolationRule::new(InterpolationFns::no_history(
            |start: &FrameA, end: &FrameA, t| FrameA(20.0 + start.0 + (end.0 - start.0) * t),
        ))
        .priority(100),
    );
    app.finish();

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(-1.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(4.0)),
                current_value: Some(FrameA(12.0)),
            },
        ))
        .id();

    app.world_mut().run_schedule(PostUpdate);

    assert_eq!(app.world().get::<FrameA>(entity), Some(&FrameA(26.0)));
}

#[test]
fn a_frame_interpolation_writes_a_sparse_set_component() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(Duration::from_millis(500));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<SparseFrame, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &SparseFrame, end: &SparseFrame, t: f32| {
            SparseFrame(start.0 + (end.0 - start.0) * t)
        }),
    ));
    app.finish();

    let entity = app
        .world_mut()
        .spawn((FrameInterpolate, SparseFrame(1.0)))
        .id();

    app.world_mut().run_schedule(FixedPostUpdate);
    assert_eq!(
        app.world()
            .get::<FrameInterpolationHistory<SparseFrame>>(entity)
            .and_then(|history| history.current_value.as_ref()),
        Some(&SparseFrame(1.0))
    );

    app.world_mut().entity_mut(entity).insert(SparseFrame(3.0));
    app.world_mut().run_schedule(FixedPostUpdate);
    let history = app
        .world()
        .get::<FrameInterpolationHistory<SparseFrame>>(entity)
        .unwrap();
    assert_eq!(history.previous_value, Some(SparseFrame(1.0)));
    assert_eq!(history.current_value, Some(SparseFrame(3.0)));
    app.world_mut().clear_trackers();
    let changed = app
        .world()
        .entity(entity)
        .get_change_ticks::<SparseFrame>()
        .unwrap()
        .changed;

    app.world_mut().run_schedule(PostUpdate);
    assert_eq!(
        app.world().get::<SparseFrame>(entity),
        Some(&SparseFrame(2.0))
    );
    assert_ne!(
        app.world()
            .entity(entity)
            .get_change_ticks::<SparseFrame>()
            .unwrap()
            .changed,
        changed
    );

    app.world_mut().run_schedule(RunFixedMainLoop);
    assert_eq!(
        app.world().get::<SparseFrame>(entity),
        Some(&SparseFrame(3.0))
    );
}

#[test]
fn a_frame_interpolation_marks_a_table_and_a_sparse_set_component_changed() {
    let mut app = App::new();
    app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs(1)));
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|_: &FrameA, end: &FrameA, _: f32| end.clone()),
    ));
    app.register_interpolation_rule::<SparseFrame, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|_: &SparseFrame, end: &SparseFrame, _: f32| end.clone()),
    ));
    app.init_resource::<ObservedFrameInterpolationChanges>();
    app.add_systems(
        PostUpdate,
        observe_frame_interpolation_changes.after(FrameInterpolationSystems::Interpolate),
    );
    app.finish();

    let entity = app
        .world_mut()
        .spawn((
            FrameInterpolate,
            FrameA(0.0),
            SparseFrame(0.0),
            FrameInterpolationHistory::<FrameA> {
                previous_value: Some(FrameA(0.0)),
                current_value: Some(FrameA(1.0)),
            },
            FrameInterpolationHistory::<SparseFrame> {
                previous_value: Some(SparseFrame(0.0)),
                current_value: Some(SparseFrame(1.0)),
            },
        ))
        .id();
    let run_and_observe = |app: &mut App| {
        *app.world_mut()
            .resource_mut::<ObservedFrameInterpolationChanges>() =
            ObservedFrameInterpolationChanges::default();
        app.world_mut().run_schedule(PostUpdate);
        *app.world().resource::<ObservedFrameInterpolationChanges>()
    };

    // Warm up the observer so the next run cannot observe the components' spawn ticks.
    run_and_observe(&mut app);

    // The same sample again writes nothing, so a stationary entity is not `Changed`.
    assert_eq!(
        run_and_observe(&mut app),
        ObservedFrameInterpolationChanges::default()
    );

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
        ObservedFrameInterpolationChanges {
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
/// frame history, the restore writes the current value, 6, and the apply at an overstep of 0.25
/// writes 2 + (6 - 2) · 0.25 = 3. At tick 7 the source holds nothing, and the value stays.
#[test]
fn a_frame_source_supplies_the_samples() {
    let mut app = App::new();
    app.add_plugins((InterpolationRegistryPlugin, FrameInterpolationPlugin));
    app.register_interpolation_rule::<FrameA, ()>(InterpolationRule::new(
        InterpolationFns::no_history(|start: &FrameA, end: &FrameA, t: f32| {
            FrameA(start.0 + (end.0 - start.0) * t)
        }),
    ));
    let source_id = app.world_mut().register_component::<FrameASamples>();
    app.world_mut()
        .resource_mut::<InterpolationRegistry>()
        .set_frame_source::<FrameA>(FrameSource::new::<FrameA>(source_id, frame_a_samples));
    app.finish();
    let mut timeline = LocalTimeline::default();
    timeline.shift(6);
    app.insert_resource(timeline);
    let mut fixed = Time::<Fixed>::from_duration(Duration::from_secs(1));
    fixed.accumulate_overstep(Duration::from_millis(250));
    app.insert_resource(fixed);
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
    let value = |app: &App| app.world().get::<FrameA>(entity).cloned();

    app.world_mut().run_schedule(FixedPostUpdate);
    assert!(
        app.world()
            .get::<FrameInterpolationHistory<FrameA>>(entity)
            .is_none()
    );
    app.world_mut().run_schedule(RunFixedMainLoop);
    assert_eq!(value(&app), Some(FrameA(6.0)));
    app.world_mut().run_schedule(PostUpdate);
    assert_eq!(value(&app), Some(FrameA(3.0)));

    app.world_mut().resource_mut::<LocalTimeline>().shift(1);
    app.world_mut().entity_mut(entity).insert(FrameA(9.0));
    app.world_mut().run_schedule(RunFixedMainLoop);
    assert_eq!(value(&app), Some(FrameA(9.0)));
}
