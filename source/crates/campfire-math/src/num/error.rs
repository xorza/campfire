use thiserror::Error;

/// Why a decimal string is not a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseNumError {
    /// Not of the form `[-]digits[.digits]`.
    #[error("not a decimal of the form [-]digits[.digits]")]
    Malformed,
    /// Outside the range of `Num`.
    #[error("outside the range of Num")]
    OutOfRange,
}
