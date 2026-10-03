use std::error::Error;
use std::fmt;

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
}

/// Why an action that passed the package load does not load into a match: what only the
/// match's tick rate decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionError {
    /// A time is too large to count in ticks.
    TimeTooLarge,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::TimeTooLarge => f.write_str("time too large to count in ticks"),
        }
    }
}

impl Error for ActionError {}
