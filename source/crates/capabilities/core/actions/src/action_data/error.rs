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

/// A rule a number of a listed effect breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EffectRule {
    /// A value, at some rank, past what a sim number holds.
    #[error("a number past what a sim number holds")]
    Overflow,
    /// A value, at some rank, below 0.
    #[error("a number below 0")]
    Negative,
    /// A duration that is no whole number of milliseconds within a `u32` at every rank, or that
    /// reads a scaling param.
    #[error("a duration of no whole milliseconds")]
    Duration,
}
