use crate::history_component::HistoryComponent;
use crate::history_seed::{insert_confirmed_history, insert_confirmed_history_diff};
use crate::interpolate::{drain_old_history, queue_history_presence, update_history_inner};
use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_phase_systems::InterpolationPhaseSystems;
use crate::interpolation_timeline::InterpolationTimeline;
use crate::update_history_cx::UpdateHistoryCx;
use bevy_app::{App, Update};
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::Interpolated;
use campfire_net_replication::ReplicationKind;
use campfire_net_timeline::NetworkTimeline;
use tracing::trace;

/// What a member that holds its confirmed history installs, once for its component: the system
/// that maintains it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConfirmedHistoryFns {
    /// How the component replicates, which one component does one way.
    pub(crate) replication: ReplicationKind,
    pub(crate) install: fn(&mut App),
}

impl ConfirmedHistoryFns {
    /// From whole replicated values.
    pub(crate) fn full<C: HistoryComponent>() -> Self {
        Self {
            replication: ReplicationKind::Full,
            install: |app| {
                app.add_systems(
                    Update,
                    Self::update_confirmed_history::<C>.in_set(InterpolationPhaseSystems::History),
                );
            },
        }
    }

    /// From replicated diffs.
    pub(crate) fn diff<C: HistoryComponent + RepliconDiffable>() -> Self {
        Self {
            replication: ReplicationKind::Diff,
            install: |app| {
                app.add_systems(
                    Update,
                    Self::update_confirmed_history_diff::<C>
                        .in_set(InterpolationPhaseSystems::History),
                );
            },
        }
    }

    /// Maintains the `ConfirmedHistory<C>` of each entity where a rule that holds `C`'s history
    /// keeps it, and seeds one where it is missing.
    ///
    /// It records completed server ticks as unchanged when appropriate, prunes old
    /// entries, and queues live `C` insertion or removal when the interpolation
    /// timeline crosses a presence boundary.
    ///
    /// Neither change marks the history changed, as [`ConfirmedHistory`] defines.
    pub(crate) fn update_confirmed_history<C: HistoryComponent>(
        timeline: Res<'_, InterpolationTimeline>,
        confirmed_ticks: Res<'_, ConfirmedTickMap>,
        holders: Res<'_, InterpolationHolders>,
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
        let ctx = UpdateHistoryCx {
            server_complete_tick: confirmed_ticks.last_confirmed_tick(),
            current_interpolate_tick: timeline.now().tick(),
        };
        let maintained = holders.delayed.history(ComponentTypeId::of::<C>());
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

    /// [`Self::update_confirmed_history`] for a component that replicates as diffs.
    ///
    /// It waits for pending diffs in `ReplicationStorage` before advancing unchanged state or
    /// pruning history past them. An entity with no [`HistoryDiffReceiver<C>`] has no pending diff.
    /// It also queues live `C` insertion or removal at interpolation-time presence boundaries,
    /// whatever the diffs' state.
    pub(crate) fn update_confirmed_history_diff<C: HistoryComponent + RepliconDiffable>(
        timeline: Res<'_, InterpolationTimeline>,
        confirmed_ticks: Res<'_, ConfirmedTickMap>,
        holders: Res<'_, InterpolationHolders>,
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
        let ctx = UpdateHistoryCx {
            server_complete_tick: confirmed_ticks.last_confirmed_tick(),
            current_interpolate_tick: timeline.now().tick(),
        };
        let storage = storage.as_deref();
        let maintained = holders.delayed.history(ComponentTypeId::of::<C>());
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

            // At a completed confirmed tick C, a diff component has exactly one of three outcomes:
            // 1. The component changed at C and the diff was materialized, so the write callback
            //    already inserted its authoritative state at C. Check ConfirmedHistory first and
            //    avoid consulting pending diff storage in this case.
            // 2. An update at or before C is still pending, so its concrete state at C is
            //    unresolved and ConfirmedHistory must not receive an entry at C yet.
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
}
