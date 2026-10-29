use crate::prediction_registration;
use bevy_app::App;
use bevy_replicon::shared::replication::diff::Diffable;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::{DiffReplicated, FullReplicated};

/// How a component of a replication kind is predicted: whole, or from diffs.
pub trait PredictionMode<C> {
    /// Adds the component's receive fns and systems, once every plugin is in.
    fn install(app: &mut App);
}

impl<C: HistoryComponent> PredictionMode<C> for FullReplicated {
    fn install(app: &mut App) {
        prediction_registration::install_prediction::<C>(app);
    }
}

impl<C: HistoryComponent + Diffable> PredictionMode<C> for DiffReplicated {
    fn install(app: &mut App) {
        prediction_registration::install_prediction_diff::<C>(app);
    }
}
