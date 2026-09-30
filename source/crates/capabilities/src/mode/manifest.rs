use std::collections::BTreeMap;

use campfire_sim::Capability;
use serde::Deserialize;

use crate::units::scalar::Scalar;
use crate::units::script_limits::ScriptLimits;

/// A package's `manifest.toml`: what it is, which engine release it targets, and, for a mode,
/// the rules its matches run by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Manifest {
    Mode(ModeManifest),
    Hero(ContentManifest),
    Spells(ContentManifest),
}

/// The manifest of a package a mode depends on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentManifest {
    pub name: String,
    pub version: String,
    /// The tag of the engine release it targets.
    pub engine: String,
}

/// A mode's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeManifest {
    pub name: String,
    pub version: String,
    /// The tag of the engine release it targets.
    pub engine: String,
    /// The capabilities its matches use; the release installs only these.
    pub capabilities: Vec<Capability>,
    pub tick_hz: TickHzRange,
    /// The playing teams, in order; their slots in that order make the player slots.
    pub teams: Vec<TeamManifest>,
    pub backends: Backends,
    /// The most any unit walks, in meters a second.
    pub max_move_speed: Scalar,
    pub script_limits: ScriptLimits,
    /// By name; in the workspace each is a path, relative to the manifest.
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// The tick rates a session may choose, in ticks a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickHzRange {
    pub min: u32,
    pub max: u32,
    pub default: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamManifest {
    pub name: String,
    pub slots: u32,
}

/// The backends of the core's collision, pathfinding and visibility. The release loads them,
/// and runs none yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backends {
    pub collision: CollisionBackend,
    pub pathfinding: PathfindingBackend,
    pub visibility: VisibilityBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionBackend {
    Circles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathfindingBackend {
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityBackend {
    GridFog,
}

/// Where a dependency is: in the workspace, a path relative to the mode's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub path: String,
}

impl Manifest {
    pub fn name(&self) -> &str {
        match self {
            Manifest::Mode(mode) => &mode.name,
            Manifest::Hero(content) | Manifest::Spells(content) => &content.name,
        }
    }

    pub fn engine(&self) -> &str {
        match self {
            Manifest::Mode(mode) => &mode.engine,
            Manifest::Hero(content) | Manifest::Spells(content) => &content.engine,
        }
    }
}
