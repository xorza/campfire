use super::*;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::internals::{replicon_app, shared_replicon_app};
use core::time::Duration;

use crate::app_interpolation_ext::AppInterpolationExt;
use crate::confirmed_history_fns::ConfirmedHistoryFns;
use crate::delayed_interpolation_plugin::DelayedInterpolationPlugin;
use crate::interpolation_fns::InterpolationFns;
use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
use crate::interpolation_registration_ext::InterpolationRegistrationExt;
use crate::interpolation_registry;
use crate::interpolation_registry::InterpolationRegistry;
use crate::interpolation_rule::InterpolationRule;
use crate::interpolation_systems::InterpolationSystems;
use crate::rule_building::install_component_rule;
use bevy_curve::{Curve, Ease, FunctionCurve, Interval};
use bevy_ecs::archetype::Archetype;
use bevy_ecs::component::Component;
use bevy_ecs::query::ArchetypeFilter;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::prelude::{Diffable as RepliconDiffable, RepliconTick};
use bevy_replicon::shared::replication::diff::diff_index::DiffIndex;
use bevy_time::TimePlugin;
use campfire_net_core::TickInstant;
use campfire_net_replication::AppComponentExt;
use core::sync::atomic::{AtomicUsize, Ordering};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Deserialize, PartialEq, Serialize)]
struct TestComp(f32);

impl Ease for TestComp {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            Self(start.0 + (end.0 - start.0) * t)
        })
    }
}

#[derive(Component, Clone, Debug, Deserialize, PartialEq, Serialize)]
struct TestComp2(f32);

#[derive(Component, Clone, Debug, Deserialize, PartialEq, Serialize)]
struct TestBundleComp<const N: usize>(f32);

#[derive(Debug, Component)]
struct SmoothRule;

#[derive(Debug, Component)]
struct HistoryOnlyRule;

#[derive(Debug, Component)]
struct NoHistoryRule;

#[derive(Debug, Component)]
struct DisabledRule;

static BUNDLE2_PRIORITY_CALLS: AtomicUsize = AtomicUsize::new(0);
static BUNDLE3_PRIORITY_CALLS: AtomicUsize = AtomicUsize::new(0);

impl RepliconDiffable for TestComp {
    type Diff = f32;

    fn apply_diff(&mut self, diff: &Self::Diff) -> Result<()> {
        self.0 = *diff;
        Ok(())
    }
}

fn idx(value: u16) -> DiffIndex {
    DiffIndex::new(value)
}

fn lerp(start: &TestComp, end: &TestComp, t: f32) -> TestComp {
    TestComp(start.0 + (end.0 - start.0) * t)
}

fn lerp2(start: &TestComp2, end: &TestComp2, t: f32) -> TestComp2 {
    TestComp2(start.0 + (end.0 - start.0) * t)
}

fn cx_lerp(_start: &TestComp, _end: &TestComp, ctx: InterpolationSampleCx) -> TestComp {
    TestComp(ctx.t + ctx.sample_delta_secs.unwrap_or_default())
}

fn marker_lerp(_start: &TestComp, _end: &TestComp, _t: f32) -> TestComp {
    TestComp(42.0)
}

fn bundle_lerp(
    start: (&TestComp, &TestComp2),
    end: (&TestComp, &TestComp2),
    t: f32,
) -> (TestComp, TestComp2) {
    (
        TestComp(100.0 + start.0.0 + (end.0.0 - start.0.0) * t),
        TestComp2(200.0 + start.1.0 + (end.1.0 - start.1.0) * t),
    )
}

fn bundle_cx_lerp(
    _start: (&TestComp, &TestComp2),
    _end: (&TestComp, &TestComp2),
    ctx: InterpolationSampleCx,
) -> (TestComp, TestComp2) {
    (
        TestComp(ctx.t),
        TestComp2(ctx.sample_delta_secs.unwrap_or_default()),
    )
}

fn bundle2_priority_lerp(
    start: (&TestComp, &TestComp2),
    end: (&TestComp, &TestComp2),
    t: f32,
) -> (TestComp, TestComp2) {
    BUNDLE2_PRIORITY_CALLS.fetch_add(1, Ordering::SeqCst);
    (
        TestComp(100.0 + start.0.0 + (end.0.0 - start.0.0) * t),
        TestComp2(200.0 + start.1.0 + (end.1.0 - start.1.0) * t),
    )
}

fn bundle3_priority_lerp(
    start: (&TestComp, &TestComp2, &TestBundleComp<3>),
    end: (&TestComp, &TestComp2, &TestBundleComp<3>),
    t: f32,
) -> (TestComp, TestComp2, TestBundleComp<3>) {
    BUNDLE3_PRIORITY_CALLS.fetch_add(1, Ordering::SeqCst);
    (
        TestComp(300.0 + start.0.0 + (end.0.0 - start.0.0) * t),
        TestComp2(400.0 + start.1.0 + (end.1.0 - start.1.0) * t),
        TestBundleComp::<3>(500.0 + start.2.0 + (end.2.0 - start.2.0) * t),
    )
}

fn bundle8_lerp(
    start: (
        &TestBundleComp<1>,
        &TestBundleComp<2>,
        &TestBundleComp<3>,
        &TestBundleComp<4>,
        &TestBundleComp<5>,
        &TestBundleComp<6>,
        &TestBundleComp<7>,
        &TestBundleComp<8>,
    ),
    end: (
        &TestBundleComp<1>,
        &TestBundleComp<2>,
        &TestBundleComp<3>,
        &TestBundleComp<4>,
        &TestBundleComp<5>,
        &TestBundleComp<6>,
        &TestBundleComp<7>,
        &TestBundleComp<8>,
    ),
    t: f32,
) -> (
    TestBundleComp<1>,
    TestBundleComp<2>,
    TestBundleComp<3>,
    TestBundleComp<4>,
    TestBundleComp<5>,
    TestBundleComp<6>,
    TestBundleComp<7>,
    TestBundleComp<8>,
) {
    (
        TestBundleComp::<1>(10.0 + start.0.0 + (end.0.0 - start.0.0) * t),
        TestBundleComp::<2>(20.0 + start.1.0 + (end.1.0 - start.1.0) * t),
        TestBundleComp::<3>(30.0 + start.2.0 + (end.2.0 - start.2.0) * t),
        TestBundleComp::<4>(40.0 + start.3.0 + (end.3.0 - start.3.0) * t),
        TestBundleComp::<5>(50.0 + start.4.0 + (end.4.0 - start.4.0) * t),
        TestBundleComp::<6>(60.0 + start.5.0 + (end.5.0 - start.5.0) * t),
        TestBundleComp::<7>(70.0 + start.6.0 + (end.6.0 - start.6.0) * t),
        TestBundleComp::<8>(80.0 + start.7.0 + (end.7.0 - start.7.0) * t),
    )
}

/// An app at `current_tick` whose rule of `TestComp` writes its history from whole values.
fn setup_app(current_tick: NetTick, send_interval_ms: u64) -> App {
    let mut app = setup_app_without_rule(current_tick, send_interval_ms);
    install_component_rule::<TestComp, ()>(
        &mut app,
        InterpolationFns::with(lerp),
        Some(ConfirmedHistoryFns::full::<TestComp>()),
        0,
    );
    app
}

/// An app at `current_tick` with no rule yet.
fn setup_app_without_rule(current_tick: NetTick, send_interval_ms: u64) -> App {
    let mut app = shared_replicon_app();
    app.add_plugins(InterpolationMarkerPlugin);
    app.world_mut().insert_resource(ConfirmedTickMap::default());
    app.world_mut()
        .insert_resource(ReplicationStorage::default());
    app.init_resource::<InterpolationRegistry>();
    add_interpolation_test_systems(&mut app);

    let mut timeline = InterpolationTimeline::default();
    timeline.set_now(TickInstant::from(current_tick));
    timeline.remote_send_interval = Duration::from_millis(send_interval_ms);
    app.insert_resource(timeline);
    app
}

#[derive(Debug)]
struct InterpolationTestAppBuilder {
    interpolation_tick: NetTick,
    tick_rate: Option<TickRate>,
}

impl InterpolationTestAppBuilder {
    fn new(interpolation_tick: NetTick) -> Self {
        Self {
            interpolation_tick,
            tick_rate: None,
        }
    }

    fn tick_rate(mut self, tick_rate: TickRate) -> Self {
        self.tick_rate = Some(tick_rate);
        self
    }

    fn build(self) -> App {
        let mut app = replicon_app();
        app.add_plugins((TimePlugin, InterpolationMarkerPlugin));
        app.insert_resource(ConfirmedTickMap::default());
        app.configure_sets(
            Update,
            (
                InterpolationSystems::Prepare,
                InterpolationSystems::Interpolate,
            )
                .chain(),
        );
        add_interpolation_test_systems(&mut app);

        if let Some(tick_rate) = self.tick_rate {
            app.insert_resource(TimelineRate(tick_rate));
        }

        let mut timeline = InterpolationTimeline::default();
        timeline.set_now(TickInstant::from(self.interpolation_tick));
        timeline.remote_send_interval = Duration::from_millis(40);
        app.insert_resource(timeline);
        app
    }
}

fn confirm_server_tick(app: &mut App, replicon_tick: u32, server_tick: NetTick) {
    let replicon_tick = RepliconTick::new(replicon_tick);
    let mut confirmed_ticks = app.world_mut().resource_mut::<ConfirmedTickMap>();
    confirmed_ticks.record(replicon_tick, server_tick);
    confirmed_ticks.record_last_confirmed_tick(replicon_tick);
}

fn set_interpolation_tick(app: &mut App, tick: NetTick) {
    app.world_mut()
        .resource_mut::<InterpolationTimeline>()
        .set_now(TickInstant::from(tick));
}

fn insert_confirmed_history(app: &mut App, entity: Entity, history: ConfirmedHistory<TestComp>) {
    app.world_mut()
        .entity_mut(entity)
        .insert((Interpolated, history));
}

/// Adds the stages each rule installs its systems in, before the rules are built.
fn add_interpolation_test_systems(app: &mut App) {
    if !app.is_plugin_added::<DelayedInterpolationPlugin>() {
        app.add_plugins(DelayedInterpolationPlugin);
    }
}

fn two_point_history() -> ConfirmedHistory<TestComp> {
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));
    history
}

fn two_point_history2() -> ConfirmedHistory<TestComp2> {
    let mut history = ConfirmedHistory::<TestComp2>::default();
    history.insert_present(NetTick(10), TestComp2(0.0));
    history.insert_present(NetTick(20), TestComp2(10.0));
    history
}

fn two_point_bundle_history<const N: usize>() -> ConfirmedHistory<TestBundleComp<N>> {
    let mut history = ConfirmedHistory::<TestBundleComp<N>>::default();
    history.insert_present(NetTick(10), TestBundleComp::<N>(0.0));
    history.insert_present(NetTick(20), TestBundleComp::<N>(10.0));
    history
}

fn use_diff_history_rule(app: &mut App) {
    install_component_rule::<TestComp, ()>(
        app,
        InterpolationFns::with(lerp),
        Some(ConfirmedHistoryFns::diff::<TestComp>()),
        100,
    );
}

fn insert_rule<C, F>(app: &mut App, fns: InterpolationFns<C>, priority: usize)
where
    C: HistoryComponent,
    F: ArchetypeFilter + 'static,
{
    let confirmed = fns
        .holds_interpolation_history()
        .then(ConfirmedHistoryFns::full::<C>);
    install_component_rule::<C, F>(app, fns, confirmed, priority);
}

#[test]
fn an_unchanged_sample_is_not_a_change() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    let entity = app.world_mut().spawn(TestComp(0.0)).id();
    insert_confirmed_history(&mut app, entity, two_point_history());
    interpolation_registry::finish(&mut app);

    let changed = |app: &App| {
        app.world()
            .entity(entity)
            .get_change_ticks::<TestComp>()
            .unwrap()
            .changed
    };
    let spawned = changed(&app);
    app.update();
    let moved = changed(&app);
    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(5.0)));
    assert_ne!(moved, spawned);
    assert_eq!(changed(&app), moved);
}

#[test]
fn a_filtered_rule_overrides_the_default_on_an_archetype_it_matches() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    QueryState::<&Archetype, With<SmoothRule>>::new(app.world_mut());
    insert_rule::<TestComp, With<SmoothRule>>(&mut app, InterpolationFns::with(marker_lerp), 100);

    let default_entity = app.world_mut().spawn(TestComp(0.0)).id();
    insert_confirmed_history(&mut app, default_entity, two_point_history());
    let filtered_entity = app.world_mut().spawn((TestComp(0.0), SmoothRule)).id();
    insert_confirmed_history(&mut app, filtered_entity, two_point_history());

    interpolation_registry::finish(&mut app);
    app.update();

    assert_eq!(
        app.world().get::<TestComp>(default_entity),
        Some(&TestComp(5.0))
    );
    assert_eq!(
        app.world().get::<TestComp>(filtered_entity),
        Some(&TestComp(42.0))
    );
}

#[test]
fn a_disabled_filtered_rule_blocks_the_broader_default_rule() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    QueryState::<&Archetype, With<DisabledRule>>::new(app.world_mut());
    insert_rule::<TestComp, With<DisabledRule>>(&mut app, InterpolationFns::disabled(), 100);

    let default_entity = app.world_mut().spawn(TestComp(0.0)).id();
    insert_confirmed_history(&mut app, default_entity, two_point_history());
    let disabled_entity = app.world_mut().spawn((TestComp(7.0), DisabledRule)).id();
    insert_confirmed_history(&mut app, disabled_entity, two_point_history());

    interpolation_registry::finish(&mut app);
    app.update();

    assert_eq!(
        app.world().get::<TestComp>(default_entity),
        Some(&TestComp(5.0))
    );
    assert_eq!(
        app.world().get::<TestComp>(disabled_entity),
        Some(&TestComp(7.0))
    );
}

/// A disabled bundle blocks lower-priority rules for every member.
#[test]
fn a_disabled_bundle_blocks_the_rules_of_its_members() {
    let mut app = setup_app(NetTick(15), 40);
    app.register_component::<TestComp2>()
        .replicate()
        .interpolate(InterpolationFns::with(lerp2));
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::disabled(),
    ));
    interpolation_registry::finish(&mut app);
    add_interpolation_test_systems(&mut app);

    let entity = app
        .world_mut()
        .spawn((TestComp(7.0), TestComp2(9.0), two_point_history2()))
        .id();
    insert_confirmed_history(&mut app, entity, two_point_history());

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(7.0)));
    assert_eq!(app.world().get::<TestComp2>(entity), Some(&TestComp2(9.0)));
}

#[test]
fn a_linear_interpolate_registers_an_ease_rule() {
    let mut app = shared_replicon_app();
    app.add_plugins(InterpolationMarkerPlugin);
    app.world_mut().insert_resource(ConfirmedTickMap::default());
    app.register_component::<TestComp>()
        .replicate()
        .interpolate(InterpolationFns::linear());
    add_interpolation_test_systems(&mut app);
    interpolation_registry::finish(&mut app);

    let mut timeline = InterpolationTimeline::default();
    timeline.set_now(TickInstant::from(NetTick(15)));
    timeline.remote_send_interval = Duration::from_millis(40);
    app.insert_resource(timeline);

    let entity = app.world_mut().spawn(TestComp(0.0)).id();
    insert_confirmed_history(&mut app, entity, two_point_history());

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(5.0)));
}

#[test]
fn a_contextual_interpolation_gets_the_sample_interval() {
    let mut app = setup_app(NetTick(15), 40);
    app.world_mut()
        .insert_resource(TimelineRate(TickRate::try_from(20).unwrap()));
    insert_rule::<TestComp, ()>(&mut app, InterpolationFns::with_cx(cx_lerp), 100);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn(TestComp(0.0)).id();
    insert_confirmed_history(&mut app, entity, two_point_history());

    interpolation_registry::finish(&mut app);
    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(1.0)));
}

#[test]
fn a_selected_history_only_rule_stops_the_default_apply() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(30));
    QueryState::<&Archetype, With<HistoryOnlyRule>>::new(app.world_mut());
    insert_rule::<TestComp, With<HistoryOnlyRule>>(
        &mut app,
        InterpolationFns::history_only().interpolate(marker_lerp),
        100,
    );
    interpolation_registry::finish(&mut app);

    let entity = app.world_mut().spawn((TestComp(7.0), HistoryOnlyRule)).id();
    insert_confirmed_history(&mut app, entity, two_point_history());

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(7.0)));
    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.get_nth_tick(history.len() - 1), Some(NetTick(30)));
}

/// A winning no-history rule blocks default history and apply work.
#[test]
fn selected_no_history_rule_suppresses_default_history_and_apply() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(30));
    QueryState::<&Archetype, With<NoHistoryRule>>::new(app.world_mut());
    insert_rule::<TestComp, With<NoHistoryRule>>(
        &mut app,
        InterpolationFns::no_history(marker_lerp),
        100,
    );

    let entity = app.world_mut().spawn((TestComp(7.0), NoHistoryRule)).id();
    insert_confirmed_history(&mut app, entity, two_point_history());

    interpolation_registry::finish(&mut app);
    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(7.0)));
    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.get_nth_tick(history.len() - 1), Some(NetTick(20)));
}

/// History-only presence does not make a lower history rule win.
#[test]
fn no_history_rule_can_win_when_only_confirmed_history_is_present() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(30));
    QueryState::<&Archetype, With<NoHistoryRule>>::new(app.world_mut());
    insert_rule::<TestComp, With<NoHistoryRule>>(
        &mut app,
        InterpolationFns::no_history(marker_lerp),
        100,
    );

    let entity = app.world_mut().spawn(NoHistoryRule).id();
    insert_confirmed_history(&mut app, entity, two_point_history());

    interpolation_registry::finish(&mut app);
    app.update();

    assert!(app.world().get::<TestComp>(entity).is_none());
    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.get_nth_tick(history.len() - 1), Some(NetTick(20)));
}

#[test]
fn a_bundle_interpolates_by_its_tuple_fn() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15))
        .tick_rate(TickRate::try_from(10).unwrap())
        .build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);
    assert_eq!(
        app.world().resource::<InterpolationRegistry>().rule_count(),
        1,
        "bundle component operations must not be registered as interpolation rules"
    );

    let entity = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(-1.0),
            TestComp2(-1.0),
            two_point_history(),
            two_point_history2(),
        ))
        .id();
    app.world_mut().clear_trackers();
    let first_changed = app
        .world()
        .entity(entity)
        .get_change_ticks::<TestComp>()
        .unwrap()
        .changed;
    let second_changed = app
        .world()
        .entity(entity)
        .get_change_ticks::<TestComp2>()
        .unwrap()
        .changed;

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(105.0)));
    assert_eq!(
        app.world().get::<TestComp2>(entity),
        Some(&TestComp2(205.0))
    );
    assert_ne!(
        app.world()
            .entity(entity)
            .get_change_ticks::<TestComp>()
            .unwrap()
            .changed,
        first_changed
    );
    assert_ne!(
        app.world()
            .entity(entity)
            .get_change_ticks::<TestComp2>()
            .unwrap()
            .changed,
        second_changed
    );
}

/// A missing live bundle member lets its standalone member rule apply.
#[test]
fn a_missing_bundle_member_lets_the_rule_of_the_other_member_apply() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>()
        .replicate()
        .interpolate(InterpolationFns::with(lerp2));
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let entity = app
        .world_mut()
        .spawn((Interpolated, TestComp2(-1.0), two_point_history2()))
        .id();

    app.update();

    assert_eq!(app.world().get::<TestComp2>(entity), Some(&TestComp2(5.0)));
    assert!(!app.world().entity(entity).contains::<TestComp>());
}

/// A selected bundle does not fall back when one history it holds is missing.
#[test]
fn a_bundle_rule_does_not_fall_back_when_one_history_it_holds_is_missing() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>()
        .replicate()
        .interpolate(InterpolationFns::with(lerp2));
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let entity = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(-1.0),
            TestComp2(-1.0),
            two_point_history2(),
        ))
        .id();

    // Apply alone, as the history pass would seed the missing history first.
    app.configure_sets(Update, InterpolationPhaseSystems::History.run_if(|| false));
    app.world_mut().run_schedule(Update);

    // The bundle holds both history initialization and application. Its
    // missing TestComp history prevents it from producing a sample, but
    // the lower-priority TestComp2 rule must not run in its place.
    assert_eq!(app.world().get::<TestComp2>(entity), Some(&TestComp2(-1.0)));
    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(-1.0)));
}

/// Independent history and apply holding across delayed bundle-member removal.
#[test]
fn a_bundle_keeps_the_histories_while_a_component_rule_applies_after_a_member_goes() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_interpolation_rule::<TestComp, ()>(InterpolationRule::new(
        InterpolationFns::with(lerp),
    ));
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let mut first = ConfirmedHistory::<TestComp>::default();
    first.insert_present(NetTick(10), TestComp(0.0));
    first.insert_present(NetTick(20), TestComp(10.0));
    let mut second = ConfirmedHistory::<TestComp2>::default();
    second.insert_present(NetTick(10), TestComp2(3.0));
    second.insert_removed(NetTick(20));
    let entity = app
        .world_mut()
        .spawn((Interpolated, TestComp(-1.0), TestComp2(-1.0), first, second))
        .id();

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(105.0)));
    assert_eq!(
        app.world().get::<TestComp2>(entity),
        Some(&TestComp2(203.0))
    );

    set_interpolation_tick(&mut app, NetTick(20));
    app.update();

    // The bundle keeps maintaining both histories, but it no longer holds
    // apply once TestComp2 is absent. The standalone TestComp rule takes
    // over immediately at the removal boundary.
    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(10.0)));
    assert!(!app.world().entity(entity).contains::<TestComp2>());
}

/// Independent history and apply holding across delayed bundle-member insertion.
#[test]
fn a_bundle_keeps_the_histories_while_a_component_rule_applies_before_a_delayed_insert() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_interpolation_rule::<TestComp, ()>(InterpolationRule::new(
        InterpolationFns::with(lerp),
    ));
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let mut first = ConfirmedHistory::<TestComp>::default();
    first.insert_present(NetTick(10), TestComp(0.0));
    first.insert_present(NetTick(20), TestComp(10.0));
    first.insert_present(NetTick(30), TestComp(20.0));
    let mut second = ConfirmedHistory::<TestComp2>::default();
    second.insert_removed(NetTick(10));
    second.insert_present(NetTick(20), TestComp2(4.0));
    second.insert_present(NetTick(30), TestComp2(8.0));
    let entity = app
        .world_mut()
        .spawn((Interpolated, TestComp(-1.0), first, second))
        .id();

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(5.0)));
    assert!(!app.world().entity(entity).contains::<TestComp2>());

    set_interpolation_tick(&mut app, NetTick(20));
    app.update();

    // History maintenance inserts TestComp2 before apply resolution runs
    // again, so the bundle takes over in the same update.
    assert_eq!(
        app.world().get::<TestComp2>(entity),
        Some(&TestComp2(204.0))
    );
    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(110.0)));
}

#[test]
fn a_contextual_bundle_interpolation_gets_the_sample_interval() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15))
        .tick_rate(TickRate::try_from(10).unwrap())
        .build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with_cx(bundle_cx_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let entity = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(-1.0),
            TestComp2(-1.0),
            two_point_history(),
            two_point_history2(),
        ))
        .id();

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(0.5)));
    assert_eq!(app.world().get::<TestComp2>(entity), Some(&TestComp2(1.0)));
}

#[test]
fn a_bundle_interpolation_inserts_each_member() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let entity = app
        .world_mut()
        .spawn((Interpolated, two_point_history(), two_point_history2()))
        .id();

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(105.0)));
    assert_eq!(
        app.world().get::<TestComp2>(entity),
        Some(&TestComp2(205.0))
    );
}

#[test]
fn a_larger_default_bundle_stops_a_smaller_bundle_it_overlaps() {
    BUNDLE2_PRIORITY_CALLS.store(0, Ordering::SeqCst);
    BUNDLE3_PRIORITY_CALLS.store(0, Ordering::SeqCst);

    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_component::<TestBundleComp<3>>().replicate();
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle2_priority_lerp),
    ));
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2, TestBundleComp<3>), ()>(
        InterpolationRule::new(InterpolationFns::with(bundle3_priority_lerp)),
    );
    interpolation_registry::finish(&mut app);

    let entity = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(-1.0),
            TestComp2(-1.0),
            TestBundleComp::<3>(-1.0),
            two_point_history(),
            two_point_history2(),
            two_point_bundle_history::<3>(),
        ))
        .id();

    app.update();

    assert_eq!(BUNDLE2_PRIORITY_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(BUNDLE3_PRIORITY_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(305.0)));
    assert_eq!(
        app.world().get::<TestComp2>(entity),
        Some(&TestComp2(405.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<3>>(entity),
        Some(&TestBundleComp::<3>(505.0))
    );
}

#[test]
fn an_earlier_member_rule_with_no_apply_stops_the_apply_of_a_bundle_of_its_priority() {
    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestComp>().replicate();
    app.register_component::<TestComp2>().replicate();
    app.register_interpolation_rule::<TestComp, ()>(
        InterpolationRule::new(InterpolationFns::history_only()).priority(2),
    );
    app.register_bundle_interpolation_rule::<(TestComp, TestComp2), ()>(InterpolationRule::new(
        InterpolationFns::with(bundle_lerp),
    ));
    interpolation_registry::finish(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(30));

    let entity = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(-1.0),
            TestComp2(-1.0),
            two_point_history(),
            two_point_history2(),
        ))
        .id();

    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(-1.0)));
    assert_eq!(app.world().get::<TestComp2>(entity), Some(&TestComp2(-1.0)));
    let first_history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(
        first_history.get_nth_tick(first_history.len() - 1),
        Some(NetTick(30))
    );
    let second_history = app
        .world()
        .get::<ConfirmedHistory<TestComp2>>(entity)
        .unwrap();
    assert_eq!(
        second_history.get_nth_tick(second_history.len() - 1),
        Some(NetTick(20))
    );
}

#[test]
fn a_bundle_of_eight_components_interpolates() {
    type Bundle8 = (
        TestBundleComp<1>,
        TestBundleComp<2>,
        TestBundleComp<3>,
        TestBundleComp<4>,
        TestBundleComp<5>,
        TestBundleComp<6>,
        TestBundleComp<7>,
        TestBundleComp<8>,
    );

    let mut app = InterpolationTestAppBuilder::new(NetTick(15)).build();
    app.register_component::<TestBundleComp<1>>().replicate();
    app.register_component::<TestBundleComp<2>>().replicate();
    app.register_component::<TestBundleComp<3>>().replicate();
    app.register_component::<TestBundleComp<4>>().replicate();
    app.register_component::<TestBundleComp<5>>().replicate();
    app.register_component::<TestBundleComp<6>>().replicate();
    app.register_component::<TestBundleComp<7>>().replicate();
    app.register_component::<TestBundleComp<8>>().replicate();
    app.register_bundle_interpolation_rule::<Bundle8, ()>(InterpolationRule::new(
        InterpolationFns::with(bundle8_lerp),
    ));
    interpolation_registry::finish(&mut app);

    let entity = app
        .world_mut()
        .spawn((
            Interpolated,
            TestBundleComp::<1>(-1.0),
            TestBundleComp::<2>(-1.0),
            TestBundleComp::<3>(-1.0),
            TestBundleComp::<4>(-1.0),
            TestBundleComp::<5>(-1.0),
            TestBundleComp::<6>(-1.0),
            TestBundleComp::<7>(-1.0),
            TestBundleComp::<8>(-1.0),
        ))
        .insert((
            two_point_bundle_history::<1>(),
            two_point_bundle_history::<2>(),
            two_point_bundle_history::<3>(),
            two_point_bundle_history::<4>(),
            two_point_bundle_history::<5>(),
            two_point_bundle_history::<6>(),
            two_point_bundle_history::<7>(),
            two_point_bundle_history::<8>(),
        ))
        .id();

    app.update();

    assert_eq!(
        app.world().get::<TestBundleComp<1>>(entity),
        Some(&TestBundleComp::<1>(15.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<2>>(entity),
        Some(&TestBundleComp::<2>(25.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<3>>(entity),
        Some(&TestBundleComp::<3>(35.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<4>>(entity),
        Some(&TestBundleComp::<4>(45.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<5>>(entity),
        Some(&TestBundleComp::<5>(55.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<6>>(entity),
        Some(&TestBundleComp::<6>(65.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<7>>(entity),
        Some(&TestBundleComp::<7>(75.0))
    );
    assert_eq!(
        app.world().get::<TestBundleComp<8>>(entity),
        Some(&TestBundleComp::<8>(85.0))
    );
}

#[test]
fn an_idle_confirmed_history_advances_to_the_latest_empty_mutate_tick() {
    let mut app = setup_app(NetTick(30), 40);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(30));

    let entity = app.world_mut().spawn(TestComp(9.5)).id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();

    let component = app.world().get::<TestComp>(entity).unwrap();
    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(component, &TestComp(10.0));
    assert_eq!(history.len(), 2);
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(20), TestComp(10.0)))
    );
    assert_eq!(
        history
            .get_nth_present(1)
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(30), TestComp(10.0)))
    );
}

#[test]
fn a_diff_history_waits_while_the_diff_of_its_completed_tick_is_pending() {
    let mut app = setup_app_without_rule(NetTick(5), 40);
    use_diff_history_rule(&mut app);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(5));

    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(0), TestComp(0.0));
    let mut receiver = HistoryDiffReceiver::<TestComp>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4.0, 5.0])
        .unwrap();

    let entity = app.world_mut().spawn((Interpolated, history)).id();
    app.world_mut()
        .resource_mut::<ReplicationStorage>()
        .insert(entity, receiver);

    interpolation_registry::finish(&mut app);
    app.update();

    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(0), TestComp(0.0)))
    );

    let receiver = app
        .world()
        .resource::<ReplicationStorage>()
        .get::<HistoryDiffReceiver<TestComp>>(entity)
        .unwrap();
    assert!(receiver.has_pending_diffs());
    assert_eq!(receiver.tick_for_cursor(Some(idx(0))), Some(NetTick(0)));
}

/// The history seeds once from the live value, at the tick its confirmation resolves to: a
/// confirmation older than every retained confirmed tick (5, below 10 and 11) resolves to the
/// oldest retained tick 10, and one within the range that nothing maps (12) leaves the history
/// empty until the next update.
#[test]
fn history_seeds_at_the_resolved_confirmation_tick() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);
    {
        let mut confirmed_ticks = app.world_mut().resource_mut::<ConfirmedTickMap>();
        confirmed_ticks.record(RepliconTick::new(10), NetTick(10));
        confirmed_ticks.record(RepliconTick::new(11), NetTick(11));
        confirmed_ticks.record(RepliconTick::new(13), NetTick(13));
    }
    let evicted = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(3.0),
            ConfirmHistory::new(RepliconTick::new(5)),
        ))
        .id();
    let unresolved = app
        .world_mut()
        .spawn((
            Interpolated,
            TestComp(3.0),
            ConfirmHistory::new(RepliconTick::new(12)),
        ))
        .id();

    interpolation_registry::finish(&mut app);
    app.update();
    app.update();

    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(evicted)
        .unwrap();
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(10), TestComp(3.0)))
    );
    assert_eq!(app.world().get::<TestComp>(evicted), Some(&TestComp(3.0)));
    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(unresolved)
        .unwrap();
    assert_eq!(history.len(), 0);
    assert_eq!(app.world().get::<TestComp>(unresolved), None);
}

/// With no diff receiver nothing is pending, so presence follows the history at the
/// interpolation tick 5: a value at tick 0 inserts the live component, and a removal at tick 3
/// removes it.
#[test]
fn diff_history_presence_does_not_need_a_receiver() {
    let mut app = setup_app_without_rule(NetTick(5), 40);
    use_diff_history_rule(&mut app);
    add_interpolation_test_systems(&mut app);

    let mut inserted_history = ConfirmedHistory::<TestComp>::default();
    inserted_history.insert_present(NetTick(0), TestComp(4.0));
    let inserted = app.world_mut().spawn((Interpolated, inserted_history)).id();
    let mut removed_history = ConfirmedHistory::<TestComp>::default();
    removed_history.insert_present(NetTick(0), TestComp(4.0));
    removed_history.insert_removed(NetTick(3));
    let removed = app
        .world_mut()
        .spawn((Interpolated, TestComp(4.0), removed_history))
        .id();

    interpolation_registry::finish(&mut app);
    app.update();

    assert_eq!(app.world().get::<TestComp>(inserted), Some(&TestComp(4.0)));
    assert_eq!(app.world().get::<TestComp>(removed), None);
}

/// A pending diff after the history's new floor does not hold the drain: at interpolation tick
/// 25, the keyframes at 0 and 10 go and 20 becomes the floor, while the diff waits at 40. The
/// drain does not mark the history changed.
#[test]
fn diff_history_drains_below_a_later_pending_diff() {
    let mut app = setup_app_without_rule(NetTick(25), 40);
    use_diff_history_rule(&mut app);
    add_interpolation_test_systems(&mut app);

    let keyframes = || {
        let mut history = ConfirmedHistory::<TestComp>::default();
        for tick in [0_u16, 10, 20, 30] {
            history.insert_present(NetTick(u32::from(tick)), TestComp(f32::from(tick)));
        }
        history
    };
    let history = keyframes();
    let mut receiver = HistoryDiffReceiver::<TestComp>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver
        .queue_diffs(NetTick(40), idx(4), vec![4.0, 5.0])
        .unwrap();
    let held_history = keyframes();
    let mut held_receiver = HistoryDiffReceiver::<TestComp>::default();
    held_receiver.record_cursor(NetTick(0), Some(idx(0)));
    held_receiver
        .queue_diffs(NetTick(15), idx(4), vec![4.0, 5.0])
        .unwrap();

    let entity = app
        .world_mut()
        .spawn((Interpolated, TestComp(0.0), history))
        .id();
    let held = app
        .world_mut()
        .spawn((Interpolated, TestComp(0.0), held_history))
        .id();
    {
        let mut storage = app.world_mut().resource_mut::<ReplicationStorage>();
        storage.insert(entity, receiver);
        storage.insert(held, held_receiver);
    }

    interpolation_registry::finish(&mut app);
    let history_changed = |app: &App| {
        app.world()
            .entity(entity)
            .get_change_ticks::<ConfirmedHistory<TestComp>>()
            .unwrap()
            .changed
    };
    let spawned = history_changed(&app);
    app.update();
    // Pruning changes no authoritative state, so it does not mark the history changed.
    assert_eq!(history_changed(&app), spawned);

    let ticks = |entity| {
        let history = app
            .world()
            .get::<ConfirmedHistory<TestComp>>(entity)
            .unwrap();
        (0..history.len())
            .map(|i| history.get_nth_tick(i).unwrap().0)
            .collect::<Vec<_>>()
    };
    assert_eq!(ticks(entity), [20, 30]);
    // The diff at 15 lands at or before the floor 20 would take, so 10 stays the floor.
    assert_eq!(ticks(held), [10, 20, 30]);
}

#[test]
fn a_confirmed_diff_history_waits_for_an_older_pending_diff() {
    let mut app = setup_app_without_rule(NetTick(6), 40);
    use_diff_history_rule(&mut app);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(6));

    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(0), TestComp(0.0));
    let mut receiver = HistoryDiffReceiver::<TestComp>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4.0, 5.0])
        .unwrap();

    let entity = app.world_mut().spawn((Interpolated, history)).id();
    app.world_mut()
        .resource_mut::<ReplicationStorage>()
        .insert(entity, receiver);

    interpolation_registry::finish(&mut app);
    app.update();

    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(0), TestComp(0.0)))
    );
    assert!(history.get_state_at(NetTick(6)).is_none());

    let receiver = app
        .world()
        .resource::<ReplicationStorage>()
        .get::<HistoryDiffReceiver<TestComp>>(entity)
        .unwrap();
    assert!(receiver.has_pending_diffs());
    assert_eq!(receiver.tick_for_cursor(Some(idx(0))), Some(NetTick(0)));

    // Materialize the missing S0 -> S3 base and the buffered S3 -> S5 diff. The next update
    // revisits the still-latest completed confirmed tick and can now add its unchanged anchor.
    let mut receiver = app
        .world_mut()
        .resource_mut::<ReplicationStorage>()
        .remove::<HistoryDiffReceiver<TestComp>>(entity)
        .unwrap();
    {
        let mut entity_mut = app.world_mut().entity_mut(entity);
        let mut history = entity_mut.get_mut::<ConfirmedHistory<TestComp>>().unwrap();
        receiver
            .queue_diffs(NetTick(3), idx(1), vec![1.0, 2.0, 3.0])
            .unwrap();
        while let Some(Ticked { tick, value }) = receiver.take_ready_update(&history).unwrap() {
            history.insert_present(tick, value);
        }
    }
    assert!(!receiver.has_pending_diffs());
    app.world_mut()
        .resource_mut::<ReplicationStorage>()
        .insert(entity, receiver);

    app.update();

    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(
        history
            .get_state_at(NetTick(6))
            .and_then(HistoryState::value),
        Some(&TestComp(5.0))
    );
}

#[test]
fn a_confirmed_history_never_moves_back() {
    let mut app = setup_app(NetTick(30), 40);
    add_interpolation_test_systems(&mut app);
    confirm_server_tick(&mut app, 1, NetTick(100));

    let entity = app.world_mut().spawn(TestComp(9.5)).id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(120), TestComp(10.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();

    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(120), TestComp(10.0)))
    );
}

#[test]
fn a_confirmed_history_keeps_its_bracketing_pair_through_a_gap_of_losses() {
    let mut app = setup_app(NetTick(25), 40);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn(TestComp(999.0)).id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));
    history.insert_present(NetTick(30), TestComp(20.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();

    let component = app.world().get::<TestComp>(entity).unwrap();
    let history = app
        .world()
        .get::<ConfirmedHistory<TestComp>>(entity)
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(20), TestComp(10.0)))
    );
    assert_eq!(
        history
            .get_nth_present(1)
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(30), TestComp(20.0)))
    );
    assert_eq!(component, &TestComp(15.0));
}

#[test]
fn a_confirmed_history_inserts_its_component_at_its_start_tick() {
    let mut app = setup_app(NetTick(9), 40);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn_empty().id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();

    assert!(!app.world().entity(entity).contains::<TestComp>());
}

#[test]
fn a_confirmed_history_removes_its_component_until_its_start_tick() {
    let mut app = setup_app(NetTick(9), 40);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn(TestComp(99.0)).id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();

    assert!(!app.world().entity(entity).contains::<TestComp>());
}

#[test]
fn a_confirmed_history_inserts_and_interpolates_at_its_start_tick() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn_empty().id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(5.0)));
}

#[test]
fn a_removal_waits_for_the_interpolation_tick_to_reach_it() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn(TestComp(99.0)).id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(10.0));
    history.insert_removed(NetTick(20));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();
    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(10.0)));

    set_interpolation_tick(&mut app, NetTick(20));
    app.update();
    assert!(!app.world().entity(entity).contains::<TestComp>());
}

#[test]
fn an_insert_after_a_removal_waits_for_its_tick() {
    let mut app = setup_app(NetTick(15), 40);
    add_interpolation_test_systems(&mut app);

    let entity = app.world_mut().spawn_empty().id();
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_removed(NetTick(10));
    history.insert_present(NetTick(20), TestComp(20.0));
    insert_confirmed_history(&mut app, entity, history);

    interpolation_registry::finish(&mut app);
    app.update();
    assert!(!app.world().entity(entity).contains::<TestComp>());

    set_interpolation_tick(&mut app, NetTick(20));
    app.update();
    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(20.0)));
}
