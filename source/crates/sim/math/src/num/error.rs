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

/// Why an integer is not a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum IntToNumError {
    /// Outside `−2³⁹ ≤ value < 2³⁹`, the integers a `Num` holds.
    #[error("outside the integers a Num holds, -2^39 to 2^39 - 1")]
    OutOfRange,
}

/// Why a float is not a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FloatToNumError {
    /// Infinite, or not a number.
    #[error("not finite")]
    NotFinite,
    /// Past `±2³⁹`, what a `Num` holds.
    #[error("outside the range of Num")]
    OutOfRange,
}
