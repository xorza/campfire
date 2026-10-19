use crate::misprediction::CheckTarget;
use crate::misprediction::rollback_needed;
use crate::prediction_history::PredictionHistory;
use crate::prediction_registry::PredictionRegistry;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use campfire_net_core::NetTick;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_replication::ComponentKind;
use campfire_net_replication::ComponentRegistry;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryDiffReceiver;
use tracing::trace;
use tracing::warn;

use crate::rollback_check_state::RollbackKind;

/// What the stages of one frame's rollback check share, as each predicted component's survey and
/// scan is its own system: the checkpoint they look at, what they report, and the rollback the
/// frame asks for.
#[derive(Resource, Debug, Default)]
pub(crate) struct RollbackScan {
    /// The local tick, once the timeline is synced: no rollback is asked for before.
    pub(crate) tick: Option<NetTick>,
    /// The newest completed checkpoint at or before the local tick.
    pub(crate) candidate: Option<NetTick>,
    /// Whether this frame's policy looks at the candidate, so the surveys report on it.
    pub(crate) surveys: bool,
    /// Whether the policy needs to know that any entity is a check target.
    pub(crate) counts_targets: bool,
    /// Whether any entity is a rollback-check target.
    pub(crate) has_check_entities: bool,
    /// The earliest unresolved predicted diff at or before the candidate.
    pub(crate) earliest_pending: Option<NetTick>,
    /// The completed checkpoint the components compare at, when the policy asks for it.
    pub(crate) scan_tick: Option<NetTick>,
    /// The first entity found mispredicted at the scan tick.
    pub(crate) mismatch: Option<Entity>,
    /// The rollback this frame asks for, and what it is for.
    pub(crate) requested: Option<NetTick>,
    pub(crate) kind: RollbackKind,
}

impl RollbackScan {
    /// The candidate checkpoint, when the surveys report on it.
    pub(crate) const fn surveyed_candidate(&self) -> Option<NetTick> {
        if self.surveys { self.candidate } else { None }
    }

    /// Keeps the earliest of the pending diff ticks reported.
    pub(crate) fn report_pending(&mut self, tick: NetTick) {
        self.earliest_pending = Some(
            self.earliest_pending
                .map_or(tick, |earliest| earliest.min(tick)),
        );
    }

    /// Asks for a rollback to `rollback_tick`, for `kind`, when it is at most `max_rollback_ticks`
    /// before the local tick and not after it; one out of those bounds asks for none.
    pub(crate) fn request(
        &mut self,
        rollback_tick: NetTick,
        max_rollback_ticks: u16,
        kind: RollbackKind,
    ) {
        let tick = self
            .tick
            .expect("a rollback is asked for once the timeline is synced");
        let delta = tick - rollback_tick;
        if delta < 0 || delta > i32::from(max_rollback_ticks) {
            warn!(
                ?rollback_tick,
                ?tick,
                "Trying to do a rollback of {delta:?} ticks. The max is {max_rollback_ticks:?}! Aborting"
            );
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "rollback_rejected",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                local_tick = tick.0,
                rollback_tick = rollback_tick.0,
                rollback_delta = delta,
                max_rollback_ticks,
                "rollback request rejected"
            );
            self.requested = None;
            return;
        }
        self.requested = Some(rollback_tick);
        self.kind = kind;
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "rollback_requested",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            local_tick = tick.0,
            rollback_tick = rollback_tick.0,
            rollback_delta = delta,
            "rollback requested"
        );
    }

    /// Reports whether an entity is a check target through `C`, a component a network replicates,
    /// when this frame's policy asks.
    pub(crate) fn survey_check_targets<C: Component>(
        component_registry: Res<'_, ComponentRegistry>,
        targets: Query<'_, '_, (), (With<PredictionHistory<C>>, CheckTarget)>,
        mut scan: ResMut<'_, Self>,
    ) {
        if scan.counts_targets
            && !scan.has_check_entities
            && component_registry.contains(&ComponentKind::of::<C>())
            && !targets.is_empty()
        {
            scan.has_check_entities = true;
        }
    }

    /// Reports the earliest diff of `C` at or before this frame's candidate checkpoint that is not
    /// resolved yet: the authoritative state is unknown from there on.
    pub(crate) fn survey_pending_diffs<C: HistoryComponent + RepliconDiffable>(
        storage: Option<Res<'_, ReplicationStorage>>,
        targets: Query<'_, '_, Entity, (With<PredictionHistory<C>>, CheckTarget)>,
        mut scan: ResMut<'_, Self>,
    ) {
        let (Some(candidate), Some(storage)) = (scan.surveyed_candidate(), storage) else {
            return;
        };
        for entity in &targets {
            if let Some(pending) = storage
                .get::<HistoryDiffReceiver<C>>(entity)
                .and_then(|receiver| receiver.earliest_pending_tick_at_or_before(candidate))
            {
                scan.report_pending(pending);
            }
        }
    }

    /// Compares `C`'s predicted state at the scan's completed checkpoint with the authoritative
    /// one, and reports the first entity that mispredicted.
    ///
    /// For a diff component, checkpoint selection has already established that no update at or
    /// before the tick remains pending, so the authoritative state there is either a received sample
    /// or, with no update at the tick, the one before it, which global completion proves carried
    /// forward. Both states are compared by reference; each entity the scan passes then keeps the
    /// carried-forward state as an exact anchor at the tick, which a later replay over the tick
    /// snaps to, as it snaps only to exact confirmed states. The anchor does not mark the history
    /// changed, as [`ConfirmedHistory`] defines.
    pub(crate) fn scan_mismatches<C: HistoryComponent>(
        component_registry: Res<'_, ComponentRegistry>,
        registry: Res<'_, PredictionRegistry>,
        mut histories: Query<
            '_,
            '_,
            (Entity, &mut ConfirmedHistory<C>, &PredictionHistory<C>),
            CheckTarget,
        >,
        mut scan: ResMut<'_, Self>,
    ) {
        let Some(confirmed_tick) = scan.scan_tick else {
            return;
        };
        if scan.mismatch.is_some() || !component_registry.contains(&ComponentKind::of::<C>()) {
            return;
        }
        let mismatch = registry.mismatch::<C>();
        for (entity, mut confirmed, predicted) in &mut histories {
            let confirmed = confirmed.bypass_change_detection();
            // No confirmed value yet, as for an entity just spawned, or no predicted state
            // retained, as for one whose local history starts after the tick, proves nothing.
            // `PredictionHistory::get` is not used: its `None` would conflate a missing sample
            // with an explicit predicted removal, which must still be checked.
            let Some(confirmed_state) = confirmed.get_state_at_or_before(confirmed_tick) else {
                continue;
            };
            let Some(predicted_state) = predicted.get_state_at_or_before(confirmed_tick) else {
                continue;
            };
            let mispredicted =
                rollback_needed(mismatch, confirmed_state.value(), predicted_state.value());
            confirmed.add_unchanged(confirmed_tick);
            if mispredicted {
                scan.mismatch = Some(entity);
                return;
            }
        }
    }
}
