use thiserror::Error;

/// A time of data, an action's or a unit type's, too large to count in ticks at the match's rate,
/// which only the rate decides, past the package load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("time too large to count in ticks")]
pub struct TimeTooLarge;
