use campfire_math::Num;
use thiserror::Error;

/// Why a grid search's reach is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GridReachError {
    /// Its gap is not above 0.
    #[error("a search's gap of {0:?} is not above 0")]
    Gap(Num),
    /// It reaches past the most steps a search takes.
    #[error("a search of {0} steps reaches past the most a search takes")]
    Steps(u32),
}
