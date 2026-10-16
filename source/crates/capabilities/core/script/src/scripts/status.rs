use derive_more::Display;

use crate::scripts::api_version::ApiVersion;

/// Whether the release runs a name, since the package API version it came in, or the script API
/// plans it.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    #[display("since {_0}")]
    Runs(ApiVersion),
    #[display("planned")]
    Planned,
}
