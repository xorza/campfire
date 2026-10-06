use thiserror::Error;

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

/// Why an action that passed the package load does not load into a match: what only the
/// match's tick rate decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ActionError {
    /// A time is too large to count in ticks.
    #[error("time too large to count in ticks")]
    TimeTooLarge,
}
