//! Building interpolation rules once every member's mode is known, and installing their typed
//! systems.
use crate::confirmed_history_fns::ConfirmedHistoryFns;
use crate::frame_interpolation_rules::install_component_frame_interpolation;
use crate::history_component::HistoryComponent;
use crate::interpolate::install_component_interpolation;
use crate::interpolation_fns::{InterpolationFns, InterpolationPipeline};
use crate::interpolation_registry::{self, InterpolationRegistry};
use crate::interpolation_rule::InterpolationRule;
use crate::interpolation_rule_component::{InterpolationRuleComponent, MemberShare};
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::member_mode::MemberMode;
use crate::pipelines::Pipelines;
use crate::registered_rule::RegisteredRule;
use bevy_app::App;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::prelude::*;
use bevy_ecs::query::ArchetypeFilter;
use campfire_net_replication::ComponentRegistry;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::ReplicationKind;

pub(crate) const SINGLE_COMPONENT_RULE_PRIORITY: usize = 1;

/// What a rule member's confirmed history is written with, as its mode says: `None` when the rule
/// holds no history, which a rule that does needs a replicated component for.
///
/// # Panics
///
/// When `C` replicates as diffs but recorded no mode, or the rule holds history of a component no
/// network replicates.
fn resolve_confirmed<C: HistoryComponent>(
    app: &mut App,
    pipeline: InterpolationPipeline,
) -> Option<ConfirmedHistoryFns> {
    let type_id = ComponentTypeId::of::<C>();
    let recorded = app
        .world()
        .resource::<InterpolationRegistry>()
        .member_mode(&type_id);
    let mode = recorded.unwrap_or_else(|| {
        let replication = app
            .world()
            .get_resource::<ComponentRegistry>()
            .and_then(|registry| registry.replication(&type_id));
        match replication {
            Some(ReplicationKind::Full) => MemberMode::full::<C>(),
            Some(ReplicationKind::Diff) => panic!(
                "{} replicates as diffs and takes part in an interpolation rule: its registration \
                 records its mode with `.interpolate(..)`",
                core::any::type_name::<C>()
            ),
            None => MemberMode::Local,
        }
    });
    pipeline.holds_interpolation_history().then(|| {
        let MemberMode::Replicated(fns) = mode else {
            panic!(
                "an interpolation rule holds the history of {}, which no network replicates",
                core::any::type_name::<C>()
            );
        };
        (fns.register_markers)(app);
        fns.confirmed
    })
}

/// Builds one member of a bundle rule, its confirmed history written as its mode says.
pub(crate) fn resolve_member<C: HistoryComponent>(
    app: &mut App,
    pipeline: InterpolationPipeline,
) -> InterpolationRuleComponent {
    let confirmed = resolve_confirmed::<C>(app, pipeline);
    member::<C>(app, confirmed, pipeline)
}

/// The member of `C`, with the frame source the registry holds for it.
fn member<C: HistoryComponent>(
    app: &mut App,
    confirmed: Option<ConfirmedHistoryFns>,
    pipeline: InterpolationPipeline,
) -> InterpolationRuleComponent {
    let frame_source = app
        .world()
        .resource::<InterpolationRegistry>()
        .frame_source(&ComponentTypeId::of::<C>())
        .cloned();
    let spec = MemberShare {
        confirmed,
        pipeline,
        frame_source,
    };
    InterpolationRuleComponent::of::<C>(app.world_mut(), spec)
}

/// Adds the rule of `members`, filtered by `F`, to the registry, and installs the systems that
/// maintain its members' histories, once for each component.
pub(crate) fn insert_rule<F: ArchetypeFilter + 'static>(
    app: &mut App,
    members: Vec<InterpolationRuleComponent>,
    priority: usize,
) -> InterpolationRuleId {
    let pipelines = Pipelines::of(app);
    for member in &members {
        let mut registry = app.world_mut().resource_mut::<InterpolationRegistry>();
        let installs_history = pipelines.delayed
            && member.confirmed.is_some_and(|confirmed| {
                registry.install_history_once(member.type_id, confirmed.replication)
            });
        let installs_frame_history = pipelines.frame
            && member.frame.is_some()
            && registry.install_frame_history_once(member.type_id);
        if installs_history && let Some(confirmed) = member.confirmed {
            (confirmed.install)(app);
        }
        if installs_frame_history && let Some(frame) = member.frame {
            (frame.install)(app, member.frame_source.clone());
        }
    }
    let rule = RegisteredRule::new::<F>(app.world().components(), members, priority);
    app.world_mut()
        .resource_mut::<InterpolationRegistry>()
        .insert_rule(rule)
}

/// Builds the rule of one component and installs its systems, whose history `confirmed` writes
/// when the rule holds it.
pub(crate) fn install_component_rule<C, F>(
    app: &mut App,
    fns: InterpolationFns<C>,
    confirmed: Option<ConfirmedHistoryFns>,
    priority: usize,
) -> InterpolationRuleId
where
    C: HistoryComponent,
    F: ArchetypeFilter + 'static,
{
    debug_assert_eq!(confirmed.is_some(), fns.holds_interpolation_history());
    let member = member::<C>(app, confirmed, fns.pipeline());
    let source = member.frame_source.clone();
    let rule_id = insert_rule::<F>(app, std::vec![member], priority);
    let pipelines = Pipelines::of(app);
    if pipelines.delayed && fns.applies_interpolation_component() {
        let interpolation = fns
            .interpolation
            .clone()
            .expect("an applying rule interpolates");
        install_component_interpolation::<C>(app, rule_id, interpolation);
    }
    if pipelines.frame && fns.applies_frame_component() {
        let interpolation = fns.interpolation.expect("an applying rule interpolates");
        install_component_frame_interpolation::<C>(app, rule_id, interpolation, source);
    }
    rule_id
}

/// Registers `rule` for component `C`, built when the app finishes its plugins.
pub(crate) fn register_component_rule<C, F>(app: &mut App, rule: InterpolationRule<C>)
where
    C: HistoryComponent,
    F: ArchetypeFilter + 'static,
{
    QueryState::<&Archetype, F>::new(app.world_mut());
    let priority = rule.priority.unwrap_or(SINGLE_COMPONENT_RULE_PRIORITY);
    let fns = rule.fns;
    interpolation_registry::open(app).defer(Box::new(move |app: &mut App| {
        let confirmed = resolve_confirmed::<C>(app, fns.pipeline());
        install_component_rule::<C, F>(app, fns, confirmed, priority);
    }));
}
