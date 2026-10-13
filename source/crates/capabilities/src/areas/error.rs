use thiserror::Error;

use crate::scripts::error::UnknownName;
use crate::values::error::TimeTooLarge;

/// Why an area type's data does not load into a match: a time too large to count in ticks at
/// its rate, or a name the match does not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum AreaError {
    #[error(transparent)]
    Time(#[from] TimeTooLarge),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}
