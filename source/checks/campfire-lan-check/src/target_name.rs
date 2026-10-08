use derive_more::Display;
use serde::Deserialize;

/// The cargo targets the check runs, by the names cargo gives them.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Deserialize)]
#[display("{}", self.name())]
pub(crate) enum TargetName {
    #[serde(rename = "campfire-server")]
    Server,
    #[serde(rename = "campfire-client")]
    Client,
    #[serde(rename = "campfire-verifier")]
    Verifier,
    #[serde(other)]
    Other,
}

impl TargetName {
    /// The package of each target the check runs, which holds a binary of the same name. CI's
    /// LAN check step builds these and the check in one command, so that they share features: a
    /// package added here goes into that command too, or CI compiles the workspace twice.
    pub(crate) const RUN: [TargetName; 3] =
        [TargetName::Server, TargetName::Client, TargetName::Verifier];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            TargetName::Server => "campfire-server",
            TargetName::Client => "campfire-client",
            TargetName::Verifier => "campfire-verifier",
            TargetName::Other => "another target",
        }
    }
}
