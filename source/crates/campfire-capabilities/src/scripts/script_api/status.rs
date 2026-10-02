use std::fmt;

use crate::scripts::api_version::ApiVersion;

/// Whether the release runs a name, since the package API version it came in, or design 08
/// plans it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Runs(ApiVersion),
    Planned,
}
/// As the reference shows it: `since <version>`, or `planned`.
impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Runs(since) => write!(f, "since {since}"),
            Status::Planned => f.write_str("planned"),
        }
    }
}
