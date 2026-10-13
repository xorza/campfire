use campfire_core::UnknownName;
use campfire_values::TimeTooLarge;
use thiserror::Error;

/// Why an area type's data does not load into a match: a time too large to count in ticks at
/// its rate, or a name the match does not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AreaError {
    #[error(transparent)]
    Time(#[from] TimeTooLarge),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}
