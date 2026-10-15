use std::error::Error;

use derive_more::Display;

/// Text that is no version.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("no version: major.minor.patch, each a decimal with no leading zero")]
pub struct NotVersion;

impl Error for NotVersion {}
