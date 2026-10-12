use thiserror::Error;

use crate::scripts::error::UnknownName;
use crate::values::error::TimeTooLarge;

/// A capability field of an action that the release runs, which does not give a value of its
/// kind at a rank: a whole number of milliseconds or of a pool the mode declares, or a range of
/// meters that is not negative, and never through a scaling param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionField {
    Range,
    Cooldown,
    Cost,
    Windup,
    /// Its charges: a count of 1 to 255, and a recharge of whole milliseconds.
    Charges,
    /// Its toggle's cost: whole amounts of the caster's pools.
    Toggle,
    /// Its channel: a length and a time between ticks of whole milliseconds, neither 0.
    Channel,
    /// Its charge's most: whole milliseconds, not 0.
    Charge,
}

/// Why an action's data does not load into a match: a time too large to count in ticks at its
/// rate, or a name the match does not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ActionError {
    #[error(transparent)]
    Time(#[from] TimeTooLarge),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}
