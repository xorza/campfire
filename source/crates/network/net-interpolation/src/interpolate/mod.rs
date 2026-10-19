use crate::history_component::HistoryComponent;
use crate::history_seed::{insert_confirmed_history, insert_confirmed_history_diff};
use crate::interpolation_fn::{ComponentInterpolation, InterpolationFn};
use crate::interpolation_owners::InterpolationOwners;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::interpolation_sample_context::InterpolationSampleContext;
use crate::interpolation_stage::InterpolationStage;
use crate::interpolation_timeline::InterpolationTimeline;
use crate::update_history_context::UpdateHistoryContext;
use bevy_app::{App, Update};
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_core::TickRate;
use campfire_net_core::Ticked;
use campfire_net_replication::ComponentKind;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::HistoryState;
use campfire_net_replication::Interpolated;
use campfire_net_replication::ReplicationCheckpointMap;
use campfire_net_timeline::NetworkTimeline;
use std::borrow::Cow;
use tracing::trace;

/// Maintains the `ConfirmedHistory<C>` of each entity where a rule that owns `C`'s history
/// keeps it, and seeds one where it is missing.
///
/// It records completed server ticks as unchanged when appropriate, prunes old
/// entries, and queues live `C` insertion or removal when the interpolation
/// timeline crosses a presence boundary.
///
/// Neither change marks the history changed, as [`ConfirmedHistory`] defines.
pub(crate) fn update_confirmed_history<C: HistoryComponent>(
    timeline: Res<'_, InterpolationTimeline>,
    checkpoints: Res<'_, ReplicationCheckpointMap>,
    owners: Res<'_, InterpolationOwners>,
    mut histories: Query<
        '_,
        '_,
        (
            Entity,
            EntityLocation,
            Has<C>,
            Option<&mut ConfirmedHistory<C>>,
        ),
        (With<Interpolated>, Or<(With<C>, With<ConfirmedHistory<C>>)>),
    >,
    mut commands: Commands<'_, '_>,
    mut deferred_apply: Deferred<'_, DeferredEntityCommands>,
) {
    let ctx = UpdateHistoryContext {
        server_complete_tick: checkpoints.last_confirmed_tick(),
        current_interpolate_tick: timeline.now().tick(),
    };
    let maintained = owners.delayed.history(ComponentKind::of::<C>());
    for (entity, location, present, history) in &mut histories {
        if !maintained.contains(location.archetype_id) {
            continue;
        }
        let Some(mut history) = history else {
            insert_confirmed_history::<C>(entity, &mut commands);
            continue;
        };
        let history = history.bypass_change_detection();
        update_history_inner::<C>(history, entity, &ctx);
        let state = history.get_state_at_or_before(ctx.current_interpolate_tick);
        queue_history_presence::<C>(&mut deferred_apply, entity, present, state);
    }
}

/// [`update_confirmed_history`] for a component that replicates as diffs.
///
/// It waits for pending diffs in `ReplicationStorage` before advancing unchanged state or pruning
/// history past them. An entity with no [`HistoryDiffReceiver<C>`] has no pending diff. It also
/// queues live `C` insertion or removal at interpolation-time presence boundaries, whatever the
/// diffs' state.
pub(crate) fn update_confirmed_history_diff<C: HistoryComponent + RepliconDiffable>(
    timeline: Res<'_, InterpolationTimeline>,
    checkpoints: Res<'_, ReplicationCheckpointMap>,
    owners: Res<'_, InterpolationOwners>,
    storage: Option<Res<'_, ReplicationStorage>>,
    mut histories: Query<
        '_,
        '_,
        (
            Entity,
            EntityLocation,
            Has<C>,
            Option<&mut ConfirmedHistory<C>>,
        ),
        (With<Interpolated>, Or<(With<C>, With<ConfirmedHistory<C>>)>),
    >,
    mut commands: Commands<'_, '_>,
    mut deferred_apply: Deferred<'_, DeferredEntityCommands>,
) {
    let ctx = UpdateHistoryContext {
        server_complete_tick: checkpoints.last_confirmed_tick(),
        current_interpolate_tick: timeline.now().tick(),
    };
    let storage = storage.as_deref();
    let maintained = owners.delayed.history(ComponentKind::of::<C>());
    for (entity_id, location, present, history) in &mut histories {
        if !maintained.contains(location.archetype_id) {
            continue;
        }
        let Some(mut history) = history else {
            insert_confirmed_history_diff::<C>(entity_id, &mut commands);
            continue;
        };
        let history = history.bypass_change_detection();
        // Most entities find their state in their history, so the receiver is looked up only
        // for one whose history lacks a tick.
        let receiver = core::cell::OnceCell::new();
        let pending_at_or_before = |tick: NetTick| {
            receiver
                .get_or_init(|| storage?.get::<HistoryDiffReceiver<C>>(entity_id))
                .is_some_and(|receiver| receiver.has_pending_diff_at_or_before(tick))
        };

        // At a completed checkpoint C, a diff component has exactly one of three outcomes:
        // 1. The component changed at C and the diff was materialized, so the write callback
        //    already inserted its authoritative state at C. Check ConfirmedHistory first and avoid
        //    consulting pending diff storage in this case.
        // 2. An update at or before C is still pending, so its concrete state at C is unresolved
        //    and ConfirmedHistory must not receive an entry at C yet.
        // 3. All diffs through C are materialized and there was no update at C, so inserting
        //    SameAsPrecedent records the final authoritative value carried forward to C.
        if let Some(server_complete_tick) = ctx.server_complete_tick
            && history.get_state_at(server_complete_tick).is_none()
            && !pending_at_or_before(server_complete_tick)
            && let Some(previous_newest_tick) = history.push_unchanged(server_complete_tick)
        {
            trace!(
                target: "campfire_net_debug::interpolation",
                kind = "confirmed_history_unchanged_advance",
                schedule = "Update",
                sample_point = "Update",
                entity = ?entity_id,
                component = ?DebugName::type_name::<C>(),
                previous_newest_tick = previous_newest_tick.0,
                server_complete_tick = server_complete_tick.0,
                history_len = history.len(),
                "advanced unchanged diff interpolation history"
            );
        }

        drain_old_history(history, ctx.current_interpolate_tick, pending_at_or_before);

        let state = history.get_state_at_or_before(ctx.current_interpolate_tick);
        queue_history_presence::<C>(&mut deferred_apply, entity_id, present, state);
    }
}

/// Installs the system that applies the component rule `rule_id` of `C`, with `interpolation`,
/// to the live `C` of each entity whose apply the rule owns. An equal value is not written, so
/// `Changed<C>` fires only when the component changes.
pub(crate) fn install_component_interpolation<C: HistoryComponent>(
    app: &mut App,
    rule_id: InterpolationRuleId,
    interpolation: InterpolationFn<C>,
) {
    let system = move |timeline: Res<'_, InterpolationTimeline>,
                       tick_rate: Option<Res<'_, TickRate>>,
                       owners: Res<'_, InterpolationOwners>,
                       mut query: Query<
        '_,
        '_,
        (EntityLocation, &mut C, &ConfirmedHistory<C>),
        With<Interpolated>,
    >| {
        let interpolation_tick = timeline.now().tick();
        let interpolation_overstep = timeline.overstep().to_f32();
        let tick_rate = tick_rate.as_deref().copied();
        let owned = owners.delayed.apply(rule_id);
        let interpolation = interpolation.component();
        for (location, live, history) in &mut query {
            if !owned.contains(location.archetype_id) {
                continue;
            }
            let Some(HistoryState::Updated(interpolated)) = sample_history_with_interpolation(
                interpolation,
                history,
                interpolation_tick,
                interpolation_overstep,
                tick_rate,
            ) else {
                continue;
            };
            trace!(
                target: "campfire_net_debug::interpolation",
                kind = "interpolation_apply",
                schedule = "Update",
                sample_point = "Update",
                component = ?DebugName::type_name::<C>(),
                interpolation_tick = interpolation_tick.0,
                interpolation_overstep,
                history_len = history.len(),
                "applied interpolation"
            );
            let _ = set_if_neq(live, interpolated);
        }
    };
    app.add_systems(Update, system.in_set(InterpolationStage::Apply));
}

/// [`DetectChangesMut::set_if_neq`] for a value that may be borrowed: it is compared in place,
/// and cloned only when it differs. It returns whether it wrote the value.
pub(crate) fn set_if_neq<C: Clone + PartialEq>(
    mut component: Mut<'_, C>,
    value: Cow<'_, C>,
) -> bool {
    if *component == *value {
        return false;
    }
    *component = value.into_owned();
    true
}

fn update_history_inner<C: Component + Clone>(
    history: &mut ConfirmedHistory<C>,
    entity: Entity,
    ctx: &UpdateHistoryContext,
) {
    // Replicon's marker fns already ran before this system. If this component received an
    // explicit update or removal at the completed server tick T, `write_history` /
    // `remove_history` already recorded that exact tick and `push_unchanged(T)` returns None.
    //
    // Therefore, when the newest confirmed state is still an Updated value older than T,
    // mutate-message completeness tells us no update/removal for this component occurred
    // through T, so we can carry the newest value forward as unchanged.
    if let Some(server_complete_tick) = ctx.server_complete_tick
        && let Some(previous_newest_tick) = history.push_unchanged(server_complete_tick)
    {
        trace!(
            target: "campfire_net_debug::interpolation",
            kind = "confirmed_history_unchanged_advance",
            schedule = "Update",
            sample_point = "Update",
            entity = ?entity,
            component = ?DebugName::type_name::<C>(),
            previous_newest_tick = previous_newest_tick.0,
            server_complete_tick = server_complete_tick.0,
            history_len = history.len(),
            "advanced unchanged interpolation history"
        );
    }

    drain_old_history(history, ctx.current_interpolate_tick, |_| false);
}

/// Drops the oldest keyframes while 3+ remain and the second-oldest, the new floor, is passed: a
/// `[behind, newest]` pair stays alive during short loss gaps instead of collapsing to one
/// keyframe. A pending diff at or before the new floor holds the floor, since its value lands
/// there.
fn drain_old_history<C: Component + Clone>(
    history: &mut ConfirmedHistory<C>,
    current_interpolate_tick: NetTick,
    pending_at_or_before: impl Fn(NetTick) -> bool,
) {
    let floor = (1..history.len().saturating_sub(1))
        .map_while(|n| {
            history
                .get_nth_tick(n)
                .filter(|&floor| floor <= current_interpolate_tick && !pending_at_or_before(floor))
        })
        .last();
    if let Some(floor) = floor {
        history.clear_until_tick(floor);
    }
}

fn queue_history_presence<C: Component + Clone>(
    deferred_apply: &mut DeferredEntityCommands,
    entity: Entity,
    present: bool,
    state: Option<&HistoryState<C>>,
) {
    // Apply the history state for the current interpolation time to the live component set:
    // insert once the add/update tick becomes visible, remove once a removal tick is reached,
    // and otherwise leave the current component value alone.
    match state {
        None | Some(HistoryState::Removed) if present => {
            deferred_apply.remove::<C>(entity);
        }
        Some(HistoryState::Updated(value)) if !present => {
            deferred_apply.insert(entity, value.clone());
        }
        _ => {}
    }
}

/// The history's state at the interpolation time: a value held in the history is borrowed, and a
/// blend of two is owned.
pub(crate) fn sample_history_with_interpolation<'a, C: Component + Clone>(
    interpolation: &ComponentInterpolation<C>,
    history: &'a ConfirmedHistory<C>,
    interpolation_tick: NetTick,
    interpolation_overstep: f32,
    tick_rate: Option<TickRate>,
) -> Option<HistoryState<Cow<'a, C>>> {
    let bracket = history_bracket(history, interpolation_tick)?;
    let HistoryState::Updated(start) = bracket.start.value else {
        return Some(HistoryState::Removed);
    };

    let Some(Ticked {
        tick: end_tick,
        value: HistoryState::Updated(end),
    }) = bracket.end
    else {
        return Some(HistoryState::Updated(Cow::Borrowed(start)));
    };

    // Clamp rather than extrapolate beyond the newest confirmed value. This
    // makes late packets converge to the freshest server state instead of
    // overshooting when motion changes direction.
    let context = InterpolationSampleContext::from_ticks(
        bracket.start.tick,
        end_tick,
        interpolation_tick,
        interpolation_overstep,
        tick_rate,
    );
    trace!(
        target: "campfire_net_debug::interpolation",
        kind = "confirmed_history_sample",
        component = ?DebugName::type_name::<C>(),
        interpolation_tick = interpolation_tick.0,
        start_tick = bracket.start.tick.0,
        end_tick = end_tick.0,
        interpolation_overstep,
        fraction = context.t,
        history_len = history.len(),
        "sampled confirmed history for interpolation"
    );
    Some(HistoryState::Updated(Cow::Owned(interpolation(
        start, end, context,
    ))))
}

/// Resolved history entry at or before a tick and its successor.
#[derive(Debug)]
pub(crate) struct HistoryBracket<'a, C> {
    pub(crate) start: Ticked<&'a HistoryState<C>>,
    pub(crate) end: Option<Ticked<&'a HistoryState<C>>>,
}

/// Returns the immediate resolved history window around `tick`.
///
/// Both component and bundle interpolation use this lookup so they agree on
/// the bracketing entries around the interpolation timeline.
pub(crate) fn history_bracket<C>(
    history: &ConfirmedHistory<C>,
    tick: NetTick,
) -> Option<HistoryBracket<'_, C>> {
    let start = history.count_at_or_before(tick).checked_sub(1)?;
    Some(HistoryBracket {
        start: history.get_nth_state(start)?,
        end: history.get_nth_state(start + 1),
    })
}

#[cfg(test)]
mod tests;
