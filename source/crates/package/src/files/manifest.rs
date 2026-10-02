use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_capabilities::{ApiVersion, CapabilitySet, ScriptLimits, Speed, TeamManifest};
use campfire_content::Language;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::files::version::Version;

/// A package's `manifest.toml`: what it is, which package API version it targets, and, for a mode,
/// the rules its matches run by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Manifest {
    Mode(ModeManifest),
    Avatar(PackageHeader),
    Loadout(PackageHeader),
    Locale(LocaleManifest),
}

/// What every manifest starts with: the package's name and version, the package API version it
/// targets, and the language of its own text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageHeader {
    pub name: String,
    pub version: Version,
    pub api: ApiVersion,
    pub language: Language,
}

/// A locale package's manifest: its header's fields, and the packages it translates, in one
/// table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocaleManifest {
    pub header: PackageHeader,
    /// By name; in the workspace each is a path, relative to the manifest.
    pub dependencies: BTreeMap<String, Dependency>,
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

impl ModeManifest {
    /// How many player slots its teams have together.
    pub fn slots(&self) -> u64 {
        self.teams.iter().map(|team| u64::from(team.slots)).sum()
    }
}

/// The tick rates a session may choose, in ticks a second: `min ≤ default ≤ max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRange {
    min: NonZeroU32,
    max: NonZeroU32,
    default: NonZeroU32,
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

    /// The fastest rate, at which a time counts the most ticks: a time that counts in ticks at
    /// it counts at every rate of the range.
    pub const fn fastest(self) -> NonZeroU32 {
        self.max
    }
}

impl<'de> Deserialize<'de> for TickRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TickRange, D::Error> {
        #[derive(Debug, Deserialize)]
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

impl Manifest {
    pub(crate) const fn header(&self) -> &PackageHeader {
        match self {
            Manifest::Mode(mode) => &mode.header,
            Manifest::Avatar(header) | Manifest::Loadout(header) => header,
            Manifest::Locale(locale) => &locale.header,
        }
    }
}

/// The flat table of a mode's manifest, its header's fields among the others.
impl<'de> Deserialize<'de> for ModeManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModeManifest, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: String,
            version: Version,
            api: ApiVersion,
            language: Language,
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
                api: fields.api,
                language: fields.language,
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

/// The flat table of a locale package's manifest, its header's fields beside its dependencies.
impl<'de> Deserialize<'de> for LocaleManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<LocaleManifest, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: String,
            version: Version,
            api: ApiVersion,
            language: Language,
            dependencies: BTreeMap<String, Dependency>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(LocaleManifest {
            header: PackageHeader {
                name: fields.name,
                version: fields.version,
                api: fields.api,
                language: fields.language,
            },
            dependencies: fields.dependencies,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_range_holds_its_default() {
        let hz = |value| NonZeroU32::new(value).unwrap();
        let range = TickRange::new(hz(20), hz(30), hz(60)).unwrap();
        assert_eq!(range.default(), hz(30));
        let held = [19, 20, 60, 61].map(|value| range.contains(hz(value)));
        assert_eq!(held, [false, true, true, false]);
        assert!(TickRange::new(hz(30), hz(30), hz(30)).is_some());
        assert!(TickRange::new(hz(40), hz(30), hz(60)).is_none());
        assert!(TickRange::new(hz(20), hz(61), hz(60)).is_none());
    }
}
