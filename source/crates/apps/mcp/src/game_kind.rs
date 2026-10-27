use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;

/// Which game a call starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GameKind {
    /// The client, with its window: with `--local`, a match of its own on a local server.
    Client,
    /// The dedicated server.
    Server,
}

/// The profile a game builds in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BuildProfile {
    /// Optimized, as a player plays: `release-dev`.
    #[default]
    Fast,
    /// The dev profile, with the debug build's asserts.
    Checked,
}

impl GameKind {
    /// The game's binary in `target`, the workspace's target directory, of `profile`.
    pub(crate) fn binary(self, target: &Path, profile: BuildProfile) -> PathBuf {
        let name = match self {
            Self::Client => "campfire-client",
            Self::Server => "campfire-server",
        };
        target
            .join(profile.dir())
            .join(format!("{name}{EXE_SUFFIX}"))
    }
}

impl BuildProfile {
    /// The driver's command that builds both games in it.
    pub(crate) const fn command(self) -> &'static str {
        match self {
            Self::Fast => "games",
            Self::Checked => "games-dev",
        }
    }

    /// Its directory in the target directory.
    const fn dir(self) -> &'static str {
        match self {
            Self::Fast => "release-dev",
            Self::Checked => "debug",
        }
    }
}
