/// Function called when comparing the confirmed component value (received from the remote) with the
/// predicted component value (from the local
/// [`PredictionHistory`](crate::prediction_history::PredictionHistory)).
///
/// In general we use [`PartialEq::ne`] by default, but you can provide your own function with
/// `PredictedComponentRegistration::mismatch_with` to customize the rollback behavior. (for
/// example, you might want to ignore small floating point differences)
pub type MismatchFn<C> = fn(confirmed: &C, predicted: &C) -> bool;
