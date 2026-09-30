use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::capability_set::CapabilitySet;
use crate::files::version::Version;
use crate::scripts::script_limits::ScriptLimits;
use crate::values::scalar::Scalar;

/// A package's `manifest.toml`: what it is, which engine release it targets, and, for a mode,
/// the rules its matches run by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Manifest {
    Mode(ModeManifest),
    Hero(PackageHeader),
    Spells(PackageHeader),
}

/// What every manifest starts with: the package's name and version, and the engine release it
/// targets.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageHeader {
    pub name: String,
    pub version: Version,
    pub engine: Version,
}

/// A mode's manifest: its header's fields, then the rules its matches run by, in one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeManifest {
    pub header: PackageHeader,
    /// The capabilities its matches use; the release installs only these.
    pub capabilities: CapabilitySet,
    pub tick_hz: TickRange,
    /// The playing teams, in order; their slots in that order make the player slots.
    pub teams: Vec<TeamManifest>,
    pub backends: Backends,
    /// The most any unit walks.
    pub max_move_speed: Speed,
    pub script_limits: ScriptLimits,
    /// By name; in the workspace each is a path, relative to the manifest.
    pub dependencies: BTreeMap<String, Dependency>,
}

/// The tick rates a session may choose, in ticks a second: `min ≤ default ≤ max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRange {
    min: NonZeroU32,
    max: NonZeroU32,
    default: NonZeroU32,
}

/// A speed in meters a second, positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Speed(Num);

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

impl TickRange {
    /// The range from `min` to `max` with its `default`; `None` unless it holds its default.
    pub const fn new(min: NonZeroU32, default: NonZeroU32, max: NonZeroU32) -> Option<TickRange> {
        if min.get() > default.get() || default.get() > max.get() {
            return None;
        }
        Some(TickRange { min, max, default })
    }

    pub const fn contains(self, hz: NonZeroU32) -> bool {
        self.min.get() <= hz.get() && hz.get() <= self.max.get()
    }

    pub const fn default(self) -> NonZeroU32 {
        self.default
    }
}

impl<'de> Deserialize<'de> for TickRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TickRange, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            min: NonZeroU32,
            max: NonZeroU32,
            default: NonZeroU32,
        }
        let Fields { min, max, default } = Fields::deserialize(deserializer)?;
        TickRange::new(min, default, max)
            .ok_or_else(|| D::Error::custom("the tick rate range does not hold its default"))
    }
}

impl Speed {
    /// `None` unless `meters_a_second` is positive.
    pub const fn new(meters_a_second: Num) -> Option<Speed> {
        if meters_a_second.to_bits() <= 0 {
            return None;
        }
        Some(Speed(meters_a_second))
    }

    /// In meters a second.
    pub const fn get(self) -> Num {
        self.0
    }
}

/// A number, or a decimal string, that is positive.
impl<'de> Deserialize<'de> for Speed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Speed, D::Error> {
        let scalar = Scalar::deserialize(deserializer)?;
        scalar
            .to_num()
            .and_then(Speed::new)
            .ok_or_else(|| D::Error::custom("a speed is a positive number"))
    }
}

impl Manifest {
    pub const fn header(&self) -> &PackageHeader {
        match self {
            Manifest::Mode(mode) => &mode.header,
            Manifest::Hero(header) | Manifest::Spells(header) => header,
        }
    }
}

/// The flat table of a mode's manifest, its header's fields among the others.
impl<'de> Deserialize<'de> for ModeManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModeManifest, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: String,
            version: Version,
            engine: Version,
            capabilities: CapabilitySet,
            tick_hz: TickRange,
            teams: Vec<TeamManifest>,
            backends: Backends,
            max_move_speed: Speed,
            script_limits: ScriptLimits,
            #[serde(default)]
            dependencies: BTreeMap<String, Dependency>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(ModeManifest {
            header: PackageHeader {
                name: fields.name,
                version: fields.version,
                engine: fields.engine,
            },
            capabilities: fields.capabilities,
            tick_hz: fields.tick_hz,
            teams: fields.teams,
            backends: fields.backends,
            max_move_speed: fields.max_move_speed,
            script_limits: fields.script_limits,
            dependencies: fields.dependencies,
        })
    }
}
