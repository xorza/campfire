use thiserror::Error;

/// A time of data, an action's or a unit type's, too large to count in ticks at the match's rate,
/// which only the rate decides, past the package load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("time too large to count in ticks")]
pub struct TimeTooLarge;

/// Why a table of names refused a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NamesError {
    #[error("its scope names it already")]
    Repeated,
    #[error("the table holds the most ids of its kind")]
    Full,
}

/// A text that writes no share: past 1, of more than nine digits, or no decimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a share is a decimal from 0 to 1 of at most 9 digits")]
pub struct ShareError;
