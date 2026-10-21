use super::*;
use crate::confirmed_history_fns::ConfirmedHistoryFns;
use crate::fixtures::interpolation_stack::InterpolationStack;
use crate::fixtures::protocol::*;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::history_receive::InterpolatedMarkerFns;
use crate::interpolate::sample_history_with_interpolation;
use crate::interpolation_fns::{InterpolationFns, InterpolationPipeline};
use crate::interpolation_phase_systems::InterpolationPhaseSystems;
use crate::interpolation_rule::InterpolationRule;
use crate::interpolation_rule_component::{InterpolationRuleComponent, MemberShare};
use crate::interpolation_sample_cx::InterpolationSampleCx;
use bevy_app::Update;
use bevy_ecs::query::ArchetypeFilter;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::prelude::EntityDiffExt;
use bevy_replicon::prelude::{RepliconTick, RuleFns};
use campfire_net_connection::NetworkTarget;
use campfire_net_core::NetTick;
use campfire_net_replication::internals::*;
use campfire_net_replication::{
    AppComponentExt, ConfirmedHistory, ConfirmedTickMap, HistoryState, Interpolated,
    InterpolatedSend, InterpolationTarget, Replicate,
};
use campfire_net_stepper::StepperSetup;
use campfire_net_stepper::{StepOrder, Stepper};

use crate::app_interpolation_ext::AppInterpolationExt;

use crate::interpolation_registration_ext::InterpolationRegistrationExt;

use bevy_replicon::shared::replication::registry::ReplicationRegistry;

use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
use crate::interpolation_plugins::InterpolationPlugins;
use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
use crate::interpolation_timeline::InterpolationTimeline;
use bevy_ecs::component::Component;
use bevy_replicon::shared::replication::registry::FnsId;
use bevy_replicon::shared::replication::registry::test_fns::TestFnsEntityExt;
use campfire_net_core::TickInstant;
use campfire_net_timeline::NetworkTimeline;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Deserialize, PartialEq, Serialize)]
struct TestComp(f32);

#[derive(Component, Clone, Debug, PartialEq)]
struct TestComp2(f32);

#[derive(Debug, Component)]
struct NoHistory;

fn lerp(start: &TestComp, end: &TestComp, t: f32) -> TestComp {
    TestComp(start.0 + (end.0 - start.0) * t)
}

fn lerp2(start: &TestComp2, end: &TestComp2, t: f32) -> TestComp2 {
    TestComp2(start.0 + (end.0 - start.0) * t)
}

fn diff_lerp(start: &CompRepliconDiff, end: &CompRepliconDiff, t: f32) -> CompRepliconDiff {
    if t < 0.5 { start.clone() } else { end.clone() }
}

/// The rule of one component `C` filtered by `F`, as the registry resolves it.
fn component_rule<C: HistoryComponent, F: ArchetypeFilter + 'static>(
    world: &mut World,
    fns: InterpolationFns<C>,
    confirmed: Option<ConfirmedHistoryFns>,
    priority: usize,
) -> RegisteredRule {
    let spec = MemberShare {
        confirmed,
        pipeline: fns.pipeline(),
        frame_source: None,
    };
    let member = InterpolationRuleComponent::of::<C>(world, spec);
    RegisteredRule::new::<F>(world.components(), std::vec![member], priority)
}

/// `history` sampled with `lerp`, as the component rule of `TestComp` samples it.
fn sample_for_rule(
    history: &ConfirmedHistory<TestComp>,
    interpolation_tick: NetTick,
    interpolation_overstep: f32,
) -> Option<HistoryState<TestComp>> {
    let interpolation = InterpolationFns::with(lerp)
        .interpolation
        .expect("the rule interpolates");
    sample_history_with_interpolation(
        interpolation.component(),
        history,
        interpolation_tick,
        interpolation_overstep,
        None,
    )
    .map(|state| match state {
        HistoryState::Updated(value) => HistoryState::Updated(value.into_owned()),
        HistoryState::Removed => HistoryState::Removed,
    })
}

fn setup_interpolation_diff_app() -> (App, FnsId) {
    let mut app = replicon_app();
    app.add_plugins((
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));
    app.insert_resource(ConfirmedTickMap::default());
    app.register_component::<CompRepliconDiff>()
        .replicate_diff()
        .interpolate(InterpolationFns::history_only());
    app.finish();

    let fns_id =
        app.world_mut()
            .resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
                let (_, fns_id) =
                    registry.register_rule_fns(world, RuleFns::<CompRepliconDiff>::new_diff());
                fns_id
            });
    (app, fns_id)
}

#[test]
fn a_diff_interpolation_applies_its_registered_sampler() {
    let mut app = replicon_app();
    app.add_plugins((
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));
    app.insert_resource(ConfirmedTickMap::default());
    app.register_component::<CompRepliconDiff>()
        .replicate_diff()
        .interpolate(InterpolationFns::with(diff_lerp));
    let mut timeline = InterpolationTimeline::default();
    timeline.set_now(TickInstant::from(NetTick(15)));
    app.insert_resource(timeline);
    app.finish();

    let mut history = ConfirmedHistory::<CompRepliconDiff>::default();
    history.insert_present(NetTick(10), CompRepliconDiff(0));
    history.insert_present(NetTick(20), CompRepliconDiff(10));
    let entity = app
        .world_mut()
        .spawn((Interpolated, CompRepliconDiff(99), history))
        .id();

    // Apply alone: the history pass would seed and advance the history first.
    app.configure_sets(Update, InterpolationPhaseSystems::History.run_if(|| false));
    app.world_mut().run_schedule(Update);

    assert_eq!(
        app.world().get::<CompRepliconDiff>(entity),
        Some(&CompRepliconDiff(10))
    );
}

/// Interpolation marker setup can be registered before replication metadata.
#[test]
fn the_marker_fns_can_come_before_the_replication_registration() {
    let mut app = replicon_app();
    app.add_plugins((
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));

    app.register_interpolation_rule::<TestComp, ()>(InterpolationRule::new(
        InterpolationFns::with(lerp),
    ));
    app.register_component::<TestComp>().replicate();
    app.finish();

    let registry = app.world().resource::<InterpolationRegistry>();
    assert!(registry.finalized);
    assert!(
        app.world()
            .resource::<InterpolatedMarkerFns>()
            .0
            .contains(&ComponentTypeId::of::<TestComp>())
    );
}

#[test]
#[should_panic(
    expected = "cannot register interpolation rules after InterpolationRegistry has been finalized"
)]
fn a_sealed_registry_refuses_a_rule() {
    let mut registry = InterpolationRegistry::default();
    let mut world = World::new();
    let fns = InterpolationFns::history_only();
    let rule = component_rule::<TestComp, ()>(
        &mut world,
        fns,
        Some(ConfirmedHistoryFns::full::<TestComp>()),
        0,
    );
    registry.finalize();
    registry.insert_rule(rule);
}

#[test]
fn a_sample_past_the_end_takes_the_newest_value() {
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(0.0));
    history.insert_present(NetTick(20), TestComp(10.0));

    assert_eq!(
        sample_for_rule(&history, NetTick(30), 0.0),
        Some(HistoryState::Updated(TestComp(10.0)))
    );
    assert_eq!(
        sample_for_rule(&history, NetTick(20), 0.5),
        Some(HistoryState::Updated(TestComp(10.0)))
    );
}

#[test]
fn a_sample_of_one_keyframe_takes_its_value() {
    let mut history = ConfirmedHistory::<TestComp>::default();
    history.insert_present(NetTick(10), TestComp(42.0));

    assert_eq!(sample_for_rule(&history, NetTick(5), 0.0), None);
    assert_eq!(
        sample_for_rule(&history, NetTick(10), 0.0),
        Some(HistoryState::Updated(TestComp(42.0)))
    );
    assert_eq!(
        sample_for_rule(&history, NetTick(50), 0.5),
        Some(HistoryState::Updated(TestComp(42.0)))
    );
}

#[test]
fn a_later_interpolated_marker_inserts_the_history_of_a_replicated_component() {
    let mut app = replicon_app();
    app.add_plugins((
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));
    app.insert_resource(ConfirmedTickMap::default());
    app.register_component::<TestComp>()
        .replicate()
        .interpolate(InterpolationFns::history_only());
    app.finish();

    let replicon_tick = RepliconTick::new(11);
    app.world_mut()
        .resource_mut::<ConfirmedTickMap>()
        .record(replicon_tick, NetTick(42));

    let entity = app
        .world_mut()
        .spawn((TestComp(2.0), ConfirmHistory::new(replicon_tick)))
        .id();
    app.world_mut().run_schedule(Update);
    assert!(
        app.world()
            .get::<ConfirmedHistory<TestComp>>(entity)
            .is_none()
    );

    app.world_mut().entity_mut(entity).insert(Interpolated);
    app.world_mut().run_schedule(Update);

    let history = app
        .world()
        .entity(entity)
        .get::<ConfirmedHistory<TestComp>>()
        .unwrap();
    assert_eq!(
        history
            .start_present()
            .map(|entry| (entry.tick, entry.value.clone())),
        Some((NetTick(42), TestComp(2.0)))
    );
    assert!(
        !app.world().entity(entity).contains::<TestComp>(),
        "live interpolated component should be removed until the interpolation timeline reaches the history start tick"
    );
}

/// A winning no-history rule prevents lower-rule history initialization.
#[test]
fn no_history_winner_does_not_initialize_lower_rule_history() {
    let mut app = replicon_app();
    app.add_plugins((
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));
    app.insert_resource(ConfirmedTickMap::default());
    app.register_component::<TestComp>()
        .replicate()
        .interpolate(InterpolationFns::with(lerp));
    app.register_interpolation_rule::<TestComp, With<NoHistory>>(
        InterpolationRule::new(InterpolationFns::no_history(lerp)).priority(100),
    );
    app.finish();

    let replicon_tick = record_confirmed_tick(&mut app, 42);
    let entity = app
        .world_mut()
        .spawn((TestComp(2.0), NoHistory, ConfirmHistory::new(replicon_tick)))
        .id();
    app.world_mut().entity_mut(entity).insert(Interpolated);
    app.world_mut().run_schedule(Update);

    assert_eq!(app.world().get::<TestComp>(entity), Some(&TestComp(2.0)));
    assert!(
        !app.world()
            .entity(entity)
            .contains::<ConfirmedHistory<TestComp>>()
    );
}

/// History and apply holding for every live/history presence combination.
#[test]
fn the_resolved_rules_hold_each_mix_of_live_and_history_members() {
    let mut world = World::new();
    let mut registry = InterpolationRegistry::default();

    let first_rule = registry.insert_rule(component_rule::<TestComp, ()>(
        &mut world,
        InterpolationFns::with(lerp),
        Some(ConfirmedHistoryFns::full::<TestComp>()),
        1,
    ));
    let second_rule = registry.insert_rule(component_rule::<TestComp2, ()>(
        &mut world,
        InterpolationFns::with(lerp2),
        Some(ConfirmedHistoryFns::full::<TestComp2>()),
        1,
    ));

    let bundle_members = std::vec![
        InterpolationRuleComponent::of::<TestComp>(
            &mut world,
            MemberShare {
                confirmed: Some(ConfirmedHistoryFns::full::<TestComp>()),
                pipeline: InterpolationPipeline::Full,
                frame_source: None,
            },
        ),
        InterpolationRuleComponent::of::<TestComp2>(
            &mut world,
            MemberShare {
                confirmed: Some(ConfirmedHistoryFns::full::<TestComp2>()),
                pipeline: InterpolationPipeline::Full,
                frame_source: None,
            },
        ),
    ];
    let bundle_rule = registry.insert_rule(RegisteredRule::new::<()>(
        world.components(),
        bundle_members,
        2,
    ));

    let both_live_no_history = world.spawn((TestComp(0.0), TestComp2(0.0))).id();
    let both_live_first_history = world
        .spawn((
            TestComp(0.0),
            TestComp2(0.0),
            ConfirmedHistory::<TestComp>::default(),
            FrameInterpolationHistory::<TestComp>::default(),
        ))
        .id();
    // This archetype represents both delayed insertion of TestComp2 before
    // its insertion tick and delayed removal after its removal tick.
    let only_first_both_histories = world
        .spawn((
            TestComp(0.0),
            ConfirmedHistory::<TestComp>::default(),
            ConfirmedHistory::<TestComp2>::default(),
            FrameInterpolationHistory::<TestComp>::default(),
            FrameInterpolationHistory::<TestComp2>::default(),
        ))
        .id();
    let only_second_both_histories = world
        .spawn((
            TestComp2(0.0),
            ConfirmedHistory::<TestComp>::default(),
            ConfirmedHistory::<TestComp2>::default(),
            FrameInterpolationHistory::<TestComp>::default(),
            FrameInterpolationHistory::<TestComp2>::default(),
        ))
        .id();
    let only_first_own_history = world
        .spawn((
            TestComp(0.0),
            ConfirmedHistory::<TestComp>::default(),
            FrameInterpolationHistory::<TestComp>::default(),
        ))
        .id();
    let only_second_own_history = world
        .spawn((
            TestComp2(0.0),
            ConfirmedHistory::<TestComp2>::default(),
            FrameInterpolationHistory::<TestComp2>::default(),
        ))
        .id();
    let histories_only = world
        .spawn((
            ConfirmedHistory::<TestComp>::default(),
            ConfirmedHistory::<TestComp2>::default(),
            FrameInterpolationHistory::<TestComp>::default(),
            FrameInterpolationHistory::<TestComp2>::default(),
        ))
        .id();
    let no_live_or_history = world.spawn_empty().id();

    let holding = |rule_id, holds_history, holds_apply| ResolvedRule {
        rule_id,
        holds_history,
        holds_apply,
    };
    let cases = std::vec![
        (
            "both live, no history yet",
            both_live_no_history,
            std::vec![holding(bundle_rule, true, true)],
        ),
        (
            "both live, one history missing",
            both_live_first_history,
            std::vec![holding(bundle_rule, true, true)],
        ),
        (
            "delayed insertion/removal of second member",
            only_first_both_histories,
            std::vec![
                holding(bundle_rule, true, false),
                holding(first_rule, false, true),
            ],
        ),
        (
            "delayed insertion/removal of first member",
            only_second_both_histories,
            std::vec![
                holding(bundle_rule, true, false),
                holding(second_rule, false, true),
            ],
        ),
        (
            "second member and its history absent",
            only_first_own_history,
            std::vec![holding(first_rule, true, true)],
        ),
        (
            "first member and its history absent",
            only_second_own_history,
            std::vec![holding(second_rule, true, true)],
        ),
        (
            "both histories retained, both live members absent",
            histories_only,
            std::vec![holding(bundle_rule, true, false)],
        ),
        (
            "no live members or histories",
            no_live_or_history,
            std::vec![],
        ),
    ];

    // History contents and change status deliberately do not appear in
    // this matrix. Holding depends only on archetype presence; sampling
    // and history maintenance interpret Updated/Removed entries later.
    let mut scratch = RuleResolutionScratch::default();
    for target in [RuleTarget::Delayed, RuleTarget::Frame] {
        for (name, entity, expected) in &cases {
            let entity_ref = world.entity(*entity);
            let archetype = entity_ref.archetype();
            let actual = registry.resolved_rules_for_archetype(
                world.components(),
                archetype,
                target,
                &mut scratch,
            );
            assert_eq!(
                actual,
                expected.as_slice(),
                "unexpected {target:?} holding for {name}"
            );
        }
    }
}

/// A bundle cannot list the same component type more than once.
#[test]
#[should_panic(expected = "interpolation bundle rules cannot contain duplicate component types")]
fn a_bundle_rule_refuses_a_component_type_twice() {
    let mut app = App::new();
    app.register_bundle_interpolation_rule::<(TestComp, TestComp), ()>(InterpolationRule::new(
        InterpolationFns::disabled(),
    ));
    finish(&mut app);
}

#[test]
fn a_diff_interpolation_buffers_a_newer_diff_until_its_older_base_arrives() {
    let (mut app, fns_id) = setup_interpolation_diff_app();
    let tick0 = record_confirmed_tick(&mut app, 0);
    let tick3 = record_confirmed_tick(&mut app, 3);
    let tick5 = record_confirmed_tick(&mut app, 5);

    let entity = app.world_mut().spawn(Interpolated).id();

    app.world_mut().entity_mut(entity).apply_write(
        diff_snapshot(0, &CompRepliconDiff(0)),
        fns_id,
        tick0,
    );

    app.world_mut()
        .entity_mut(entity)
        .apply_write(diff_message(5, &[4, 5]), fns_id, tick5);
    {
        let entity_ref = app.world().entity(entity);
        let history = entity_ref
            .get::<ConfirmedHistory<CompRepliconDiff>>()
            .unwrap();
        assert!(history.get_state_at(NetTick(5)).is_none());
    }

    app.world_mut()
        .entity_mut(entity)
        .apply_write(diff_message(3, &[1, 2, 3]), fns_id, tick3);

    let entity_ref = app.world().entity(entity);
    let history = entity_ref
        .get::<ConfirmedHistory<CompRepliconDiff>>()
        .unwrap();
    assert_eq!(
        history
            .get_state_at(NetTick(3))
            .and_then(HistoryState::value),
        Some(&CompRepliconDiff(3))
    );
    assert_eq!(
        history
            .get_state_at(NetTick(5))
            .and_then(HistoryState::value),
        Some(&CompRepliconDiff(5))
    );
}

fn frame_only_app() -> App {
    let mut app = replicon_app();
    app.add_plugins((
        InterpolationMarkerPlugin,
        InterpolationRegistryPlugin,
        InterpolationPlugins,
    ));
    app
}

fn frame_only_rule_for<C: Component>(registry: &InterpolationRegistry) -> &RegisteredRule {
    let type_id = ComponentTypeId::of::<C>();
    registry
        .rules
        .iter()
        .find(|rule| rule.members().any(|member| member == type_id))
        .expect("frame-only rule should be registered")
}

fn assert_frame_only_rule<C: Component>(registry: &InterpolationRegistry) {
    let rule = frame_only_rule_for::<C>(registry);
    // The rule holds the frame history, and no delayed-interpolation history.
    let member = rule
        .members
        .iter()
        .find(|member| member.type_id == ComponentTypeId::of::<C>())
        .expect("rule should contain the component");
    assert!(member.confirmed.is_none());
    assert!(member.frame.is_some());
}

#[test]
fn a_chained_interpolate_with_registers_a_frame_only_rule() {
    let mut app = frame_only_app();
    app.register_component::<TestComp>()
        .replicate()
        .interpolate(InterpolationFns::no_history(lerp));
    app.finish();

    let registry = app.world().resource::<InterpolationRegistry>();
    assert!(registry.is_interpolated::<TestComp>());
    assert_frame_only_rule::<TestComp>(registry);
}

#[test]
fn a_chained_contextual_interpolate_registers_a_frame_only_rule() {
    fn cx_lerp(start: &TestComp, end: &TestComp, ctx: InterpolationSampleCx) -> TestComp {
        lerp(start, end, ctx.t)
    }

    let mut app = frame_only_app();
    app.register_component::<TestComp>()
        .replicate()
        .interpolate(InterpolationFns::no_history_with_cx(cx_lerp));
    app.finish();

    let registry = app.world().resource::<InterpolationRegistry>();
    assert!(registry.is_interpolated::<TestComp>());
    assert_frame_only_rule::<TestComp>(registry);
}

#[test]
fn a_chained_diff_interpolate_registers_a_frame_only_rule() {
    let mut app = frame_only_app();
    app.insert_resource(ConfirmedTickMap::default());
    app.register_component::<CompRepliconDiff>()
        .replicate_diff()
        .interpolate(InterpolationFns::no_history(diff_lerp));
    app.finish();

    let registry = app.world().resource::<InterpolationRegistry>();
    assert!(registry.is_interpolated::<CompRepliconDiff>());
    assert_frame_only_rule::<CompRepliconDiff>(registry);
}

/// A bundle member that replicates as diffs writes its confirmed history from diffs: the mode its
/// own registration recorded, which the bundle's generic code cannot name.
#[test]
fn a_diff_bundle_member_writes_history_from_diffs() {
    let mut app = frame_only_app();
    app.insert_resource(ConfirmedTickMap::default());
    app.register_component::<TestComp>().replicate();
    app.register_component::<CompRepliconDiff>()
        .replicate_diff()
        .interpolate(InterpolationFns::disabled());
    app.register_bundle_interpolation_rule::<(TestComp, CompRepliconDiff), ()>(
        InterpolationRule::new(InterpolationFns::history_only()),
    );
    app.finish();
    let fns_id =
        app.world_mut()
            .resource_scope(|world, mut registry: Mut<'_, ReplicationRegistry>| {
                let (_, fns_id) =
                    registry.register_rule_fns(world, RuleFns::<CompRepliconDiff>::new_diff());
                fns_id
            });
    let tick0 = record_confirmed_tick(&mut app, 0);
    let tick3 = record_confirmed_tick(&mut app, 3);

    let entity = app.world_mut().spawn(Interpolated).id();
    app.world_mut().entity_mut(entity).apply_write(
        diff_snapshot(0, &CompRepliconDiff(7)),
        fns_id,
        tick0,
    );
    app.world_mut()
        .entity_mut(entity)
        .apply_write(diff_message(2, &[8, 9]), fns_id, tick3);

    let history = app
        .world()
        .get::<ConfirmedHistory<CompRepliconDiff>>(entity)
        .unwrap();
    assert_eq!(
        history
            .get_state_at(NetTick(0))
            .and_then(HistoryState::value),
        Some(&CompRepliconDiff(7))
    );
    // Diffs 1 and 2 follow snapshot 0, and each replaces the value: 9 at tick 3.
    assert_eq!(
        history
            .get_state_at(NetTick(3))
            .and_then(HistoryState::value),
        Some(&CompRepliconDiff(9))
    );
}

#[test]
#[should_panic(expected = "replicates as diffs and takes part in an interpolation rule")]
fn a_diff_bundle_member_without_a_recorded_mode_panics() {
    let mut app = frame_only_app();
    app.register_component::<TestComp>().replicate();
    app.register_component::<CompRepliconDiff>()
        .replicate_diff();
    app.register_bundle_interpolation_rule::<(TestComp, CompRepliconDiff), ()>(
        InterpolationRule::new(InterpolationFns::history_only()),
    );
    app.finish();
}

#[test]
#[should_panic(expected = "which no network replicates")]
fn a_history_rule_on_an_unreplicated_component_panics() {
    let mut app = frame_only_app();
    app.register_interpolation_rule::<TestComp2, ()>(InterpolationRule::new(
        InterpolationFns::history_only(),
    ));
    app.finish();
}

/// An unreplicated component takes a rule that holds no history, as frame interpolation does.
#[test]
fn an_unreplicated_component_takes_a_frame_only_rule() {
    let mut app = frame_only_app();
    app.register_component::<TestComp2>()
        .interpolate(InterpolationFns::no_history(lerp2));
    app.finish();

    assert_frame_only_rule::<TestComp2>(app.world().resource::<InterpolationRegistry>());
}

#[test]
#[should_panic(expected = "registers for interpolation in two modes")]
fn a_component_interpolates_one_way() {
    let mut app = frame_only_app();
    app.register_component::<CompRepliconDiff>()
        .replicate_diff()
        .interpolate(InterpolationFns::disabled());
    app.register_component::<CompRepliconDiff>()
        .interpolate(InterpolationFns::disabled());
}

#[test]
#[should_panic(
    expected = "cannot register interpolation rules after InterpolationRegistry has been finalized"
)]
fn a_rule_after_the_app_finishes_panics() {
    let mut app = frame_only_app();
    app.finish();
    app.register_interpolation_rule::<TestComp2, ()>(InterpolationRule::new(
        InterpolationFns::no_history(lerp2),
    ));
}

#[test]
fn a_history_only_component_gets_a_confirmed_history() {
    let mut stepper = Stepper::new(InterpolationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
            CompCustomInterp(1.0),
        ))
        .id();

    let server_entity_ref = stepper.server_world().entity(server_entity);
    assert!(server_entity_ref.contains::<InterpolatedSend>());
    assert!(
        !server_entity_ref.contains::<Interpolated>(),
        "an interpolation target must not mark the authoritative sender as interpolated"
    );

    stepper.frame_step(PROPAGATION_FRAMES);
    let client_entity = stepper.client_copy(0, server_entity).unwrap();
    {
        let client_entity_ref = stepper.client_world(0).entity(client_entity);
        assert!(
            client_entity_ref.get::<Interpolated>().is_some(),
            "entity should be interpolated on the client"
        );
        assert!(
            client_entity_ref.get::<InterpolatedSend>().is_none(),
            "the sender-only interpolation marker must not be materialized on the receiver"
        );
        let history = client_entity_ref
            .get::<ConfirmedHistory<CompCustomInterp>>()
            .expect("the initial component should be written directly to interpolation history");
        assert_eq!(
            history.newest_present().map(|entry| entry.value),
            Some(&CompCustomInterp(1.0)),
            "InterpolatedSend should select the history receive function in the initial update"
        );
    }

    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .insert(CompCustomInterp(2.0));
    stepper.step(2, stepper.frame_interval(), StepOrder::ServerFirst);

    let client_entity_ref = stepper.client_world(0).entity(client_entity);
    let history = client_entity_ref
        .get::<ConfirmedHistory<CompCustomInterp>>()
        .expect(
            "custom-interpolated components should get ConfirmedHistory on interpolated entities",
        );
    assert!(
        history.start_present().is_some(),
        "custom-interpolated history should contain at least one confirmed update"
    );
}

/// The end-to-end interpolation path for Replicon diff components:
/// server-side `apply_diff` produces diff replication, and the client stores
/// the materialized value in `ConfirmedHistory`.
#[test]
fn a_diff_interpolation_records_its_confirmed_history_from_replicon_diffs() {
    let mut stepper = Stepper::new(InterpolationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
            CompRepliconDiff(0),
        ))
        .id();
    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .apply_diff::<CompRepliconDiff>(0)
        .unwrap();

    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
    let client_entity = stepper.client_copy(0, server_entity).unwrap();
    assert!(
        stepper
            .client_world(0)
            .entity(client_entity)
            .contains::<Interpolated>()
    );
    assert_eq!(
        newest_confirmed_value(stepper.client_app_mut(0), client_entity),
        Some(0)
    );

    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .apply_diff::<CompRepliconDiff>(1)
        .unwrap();
    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);

    assert_eq!(
        newest_confirmed_value(stepper.client_app_mut(0), client_entity),
        Some(1)
    );
}

impl InterpolationRegistry {
    pub(crate) const fn rule_count(&self) -> usize {
        self.rules.len()
    }
}
