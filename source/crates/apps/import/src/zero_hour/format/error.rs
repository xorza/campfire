use thiserror::Error;

/// Why a float of the game's files is not a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum FloatError {
    /// Infinite, or not a number.
    #[error("not finite")]
    NotFinite,
    /// Past `±2³⁹`, what a `Num` holds.
    #[error("outside the range of Num")]
    OutOfRange,
}

/// Bytes of the original that end inside a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("it ends inside a field")]
pub(crate) struct Short;
