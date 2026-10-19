//! The surveys and scans the rollback check runs for each predicted component, typed, between
//! its policy stages.

use bevy_ecs::prelude::*;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_utils::prelude::DebugName;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_replication::Predicted;
use tracing::{debug, trace};

use crate::disable_rollback::DisableRollback;
use crate::locally_predicted::LocallyPredicted;
use crate::mismatch_fn::MismatchFn;
use crate::prediction_despawned::PredictionDespawned;

/// The entities a rollback check compares: predicted ones whose confirmations the server sends,
/// neither locally predicted nor kept out of rollbacks, with a predicted despawn or not.
pub(crate) type CheckTarget = (
    With<Predicted>,
    With<ConfirmHistory>,
    Without<LocallyPredicted>,
    Without<DisableRollback>,
    Allow<PredictionDespawned>,
);

/// Whether the confirmed and the predicted state of `C` differ, as `mismatch` compares
/// two present values.
pub(crate) fn needs_rollback<C: HistoryComponent>(
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
