use std::error::Error;

use derive_more::Display;

/// Text that is no map's name.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[display("no map's name: a lowercase letter, then lowercase letters, digits and underscores")]
pub struct NotMapName;

impl Error for NotMapName {}
