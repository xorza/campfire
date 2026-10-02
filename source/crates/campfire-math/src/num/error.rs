use std::error::Error;
use std::fmt;

/// Why a decimal string is not a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseNumError {
    /// Not of the form `[-]digits[.digits]`.
    Malformed,
    /// Outside the range of `Num`.
    OutOfRange,
}

impl fmt::Display for ParseNumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseNumError::Malformed => "not a decimal of the form [-]digits[.digits]",
            ParseNumError::OutOfRange => "outside the range of Num",
        })
    }
}

impl Error for ParseNumError {}
