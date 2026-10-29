use crate::interpolate::{drain_old_history, queue_history_presence};
use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_phase_systems::InterpolationPhaseSystems;
use crate::interpolation_timeline::InterpolationTimeline;
use crate::pending_diffs::{Diffs, PendingDiffs, WholeValues};
use bevy_app::{App, Update};
use bevy_ecs::entity::EntityLocation;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_replication::ComponentTypeId;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryComponent;
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
                    Self::update_confirmed_history::<C, WholeValues>
                        .in_set(InterpolationPhaseSystems::History),
                );
            },
        }
    }

    /// From replicated diffs.
    pub(crate) fn diff<C: HistoryComponent + Diffable>() -> Self {
        Self {
            replication: ReplicationKind::Diff,
            install: |app| {
                app.add_systems(
                    Update,
                    Self::update_confirmed_history::<C, Diffs>
                        .in_set(InterpolationPhaseSystems::History),
                );
            },
        }
    }

    /// Maintains the `ConfirmedHistory<C>` of each entity where a rule that holds `C`'s history
    /// keeps it, and seeds one where it is missing.
    ///
    /// At the last completed server tick it carries the newest state forward as unchanged, unless
    /// a diff waits to land there; it prunes the entries the interpolation tick passed, but for one
    /// a waiting diff lands at; and it queues live `C` insertion or removal when the interpolation
    /// timeline crosses a presence boundary.
    ///
    /// Neither change marks the history changed, as [`ConfirmedHistory`] defines.
    fn update_confirmed_history<C: HistoryComponent, P: PendingDiffs<C>>(
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
        let server_complete_tick = confirmed_ticks.last_confirmed_tick();
        let interpolation_tick = timeline.now().tick();
        let storage = storage.as_deref();
        let maintained = holders.history(ComponentTypeId::of::<C>());
        for (entity, location, present, history) in &mut histories {
            if !maintained.contains(location.archetype_id) {
                continue;
            }
            let Some(mut history) = history else {
                P::seed(entity, &mut commands);
                continue;
            };
            let history = history.bypass_change_detection();
            let pending_at_or_before = P::pending(storage, entity);

            // Replicon's marker fns ran before this system, so at the completed server tick T the
            // component has one of three outcomes:
            // 1. It changed at T, and the history holds its update or removal there.
            // 2. A diff at or before T waits for its base, so its state at T is not known yet and
            //    the history takes no entry there.
            // 3. Neither, so the message's completeness proves that the newest state held through
            //    T, and the history carries it forward as unchanged.
            if let Some(server_complete_tick) = server_complete_tick
                && history.get_state_at(server_complete_tick).is_none()
                && !pending_at_or_before(server_complete_tick)
                && let Some(previous_newest_tick) = history.push_unchanged(server_complete_tick)
            {
                trace!(
                    target: "campfire_net_debug::interpolation",
                    kind = "confirmed_history_unchanged_advance",
                    entity = ?entity,
                    component = ?DebugName::type_name::<C>(),
                    previous_newest_tick = previous_newest_tick.0,
                    server_complete_tick = server_complete_tick.0,
                    history_len = history.len(),
                    "advanced unchanged interpolation history"
                );
            }

            drain_old_history(history, interpolation_tick, pending_at_or_before);

            let state = history.get_state_at_or_before(interpolation_tick);
            queue_history_presence::<C>(&mut deferred_apply, entity, present, state);
        }
    }
}
