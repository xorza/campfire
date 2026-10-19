use crate::history_component::HistoryComponent;
use crate::interpolation_fn::{ComponentInterpolation, InterpolationFn};
use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_rule_id::InterpolationRuleId;
use crate::interpolation_sample_cx::InterpolationSampleCx;
use crate::interpolation_stage::InterpolationStage;
use crate::interpolation_timeline::InterpolationTimeline;
use crate::update_history_cx::UpdateHistoryCx;
use bevy_app::{App, Update};
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_core::TickRate;
use campfire_net_core::Ticked;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryState;
use campfire_net_replication::Interpolated;
use campfire_net_timeline::NetworkTimeline;
use std::borrow::Cow;
use tracing::trace;

/// Installs the system that applies the component rule `rule_id` of `C`, with `interpolation`,
/// to the live `C` of each entity whose apply the rule holds. An equal value is not written, so
/// `Changed<C>` fires only when the component changes.
pub(crate) fn install_component_interpolation<C: HistoryComponent>(
    app: &mut App,
    rule_id: InterpolationRuleId,
    interpolation: InterpolationFn<C>,
) {
    let system = move |timeline: Res<'_, InterpolationTimeline>,
                       tick_rate: Option<Res<'_, TickRate>>,
                       holders: Res<'_, InterpolationHolders>,
                       mut query: Query<
        '_,
        '_,
        (EntityLocation, &mut C, &ConfirmedHistory<C>),
        With<Interpolated>,
    >| {
        let interpolation_tick = timeline.now().tick();
        let interpolation_overstep = timeline.overstep().to_f32();
        let tick_rate = tick_rate.as_deref().copied();
        let held = holders.delayed.apply(rule_id);
        let interpolation = interpolation.component();
        for (location, live, history) in &mut query {
            if !held.contains(location.archetype_id) {
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

pub(crate) fn update_history_inner<C: Component + Clone>(
    history: &mut ConfirmedHistory<C>,
    entity: Entity,
    ctx: &UpdateHistoryCx,
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
pub(crate) fn drain_old_history<C: Component + Clone>(
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

pub(crate) fn queue_history_presence<C: Component + Clone>(
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
    let cx = InterpolationSampleCx::from_ticks(
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
        fraction = cx.t,
        history_len = history.len(),
        "sampled confirmed history for interpolation"
    );
    Some(HistoryState::Updated(Cow::Owned(interpolation(
        start, end, cx,
    ))))
}

/// Resolved history entry at or before a tick and its successor.
#[derive(Debug)]
pub(crate) struct HistoryBracket<'a, C> {
    pub(crate) start: Ticked<&'a HistoryState<C>>,
    pub(crate) end: Option<Ticked<&'a HistoryState<C>>>,
}

/// The immediate resolved history window around `tick`.
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
