use crate::value32::{Meaning, Value32};

/// The hash of the whole simulated state, which the sim computes and the session log's
/// checkpoints and result carry.
pub type StateHash = Value32<StateHashMeaning>;

/// The meaning of a `StateHash`.
#[derive(Debug)]
pub enum StateHashMeaning {}

impl Meaning for StateHashMeaning {
    const NAME: &'static str = "StateHash";
}
