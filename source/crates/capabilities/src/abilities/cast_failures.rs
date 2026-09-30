use campfire_sim::StableId;

use crate::abilities::error::CastError;

/// The casts of the running tick that failed to resolve. A failed cast changed nothing; the list
/// is for the host to see. Not state: it empties each tick. A non-send resource, as a script's
/// raised value is not `Send`.
#[derive(Debug, Default)]
pub struct CastFailures(pub(crate) Vec<CastFailure>);

/// A cast that failed, and why.
#[derive(Debug, Clone)]
pub struct CastFailure {
    pub caster: StableId,
    pub error: CastError,
}

impl CastFailures {
    pub fn get(&self) -> &[CastFailure] {
        &self.0
    }
}
