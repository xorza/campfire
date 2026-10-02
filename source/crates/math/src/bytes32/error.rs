use std::error::Error;
use std::fmt;

/// Text that is not 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotHex;

impl fmt::Display for NotHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not 64 lowercase hex digits")
    }
}

impl Error for NotHex {}
