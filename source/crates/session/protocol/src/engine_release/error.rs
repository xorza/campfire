use thiserror::Error;

/// Text that is no engine release.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("no engine release: major.minor.patch, each a decimal with no leading zero")]
pub struct NotRelease;
