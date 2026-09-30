use std::fmt;

use serde::Deserialize;

/// The cargo targets the check runs, by the names cargo gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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
    /// The package of each target the check runs, which holds a binary of the same name.
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

impl fmt::Display for TargetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
