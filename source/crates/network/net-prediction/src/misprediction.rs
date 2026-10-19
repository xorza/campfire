//! The surveys and scans the rollback check runs for each predicted component, typed, between
//! its policy stages.

use bevy_ecs::prelude::*;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_replication::ComponentKind;
use campfire_net_replication::ComponentRegistry;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::Predicted;
use tracing::{debug, trace};

use crate::disable_rollback::DisableRollback;
use crate::locally_predicted::LocallyPredicted;
use crate::mismatch_fn::MismatchFn;
use crate::prediction_despawned::PredictionDespawned;
use crate::prediction_history::PredictionHistory;
use crate::prediction_registry::PredictionRegistry;
use crate::rollback_scan::RollbackScan;

/// The entities a rollback check compares: predicted ones whose confirmations the server sends,
/// neither locally predicted nor kept out of rollbacks, with a predicted despawn or not.
pub(crate) type CheckTarget = (
    With<Predicted>,
    With<ConfirmHistory>,
    Without<LocallyPredicted>,
    Without<DisableRollback>,
    Allow<PredictionDespawned>,
);

/// Reports whether an entity is a check target through `C`, a component a network replicates,
/// when this frame's policy asks.
pub(crate) fn survey_check_targets<C: Component>(
    component_registry: Res<'_, ComponentRegistry>,
    targets: Query<'_, '_, (), (With<PredictionHistory<C>>, CheckTarget)>,
    mut scan: ResMut<'_, RollbackScan>,
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
    mut scan: ResMut<'_, RollbackScan>,
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
    mut scan: ResMut<'_, RollbackScan>,
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

/// Whether the confirmed and the predicted state of `C` differ, as `mismatch` compares
/// two present values.
fn rollback_needed<C: HistoryComponent>(
    mismatch: MismatchFn<C>,
    confirmed: Option<&C>,
    predicted: Option<&C>,
) -> bool {
    match (confirmed, predicted) {
        (Some(c), Some(p)) => {
            let should = mismatch(c, p);
            if should {
                debug!(
                    "Should Rollback! Confirmed value {c:?} is different from predicted value {p:?}",
                );
                trace!(
                    target: "campfire_net_debug::prediction",
                    kind = "rollback_value_mismatch",
                    component = ?DebugName::type_name::<C>(),
                    confirmed = ?c,
                    predicted = ?p,
                    "confirmed value differs from prediction history"
                );
            }
            should
        }
        (Some(c), None) => {
            debug!(
                "Should Rollback! Confirmed component exists ({c:?}), but predicted value does not exists",
            );
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "rollback_missing_on_predicted",
                component = ?DebugName::type_name::<C>(),
                confirmed = ?c,
                "confirmed component missing from prediction history"
            );
            true
        }
        (None, Some(p)) => {
            debug!(
                "Should Rollback! Confirmed component does not exist, but predicted value exists ({p:?})",
            );
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "rollback_missing_on_confirmed",
                component = ?DebugName::type_name::<C>(),
                predicted = ?p,
                "predicted component missing from confirmed state"
            );
            true
        }
        (None, None) => false,
    }
}
