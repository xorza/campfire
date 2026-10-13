use std::error::Error;

use derive_more::Display;

/// Text that is not 64 lowercase hex digits.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("not 64 lowercase hex digits")]
pub struct NotHex;

impl Error for NotHex {}
