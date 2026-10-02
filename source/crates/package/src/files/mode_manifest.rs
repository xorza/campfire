use std::collections::BTreeMap;

use campfire_capabilities::{ApiVersion, CapabilitySet, ScriptLimits, Speed, TeamManifest};
use campfire_content::Language;
use serde::{Deserialize, Deserializer};

use crate::files::backends::Backends;
use crate::files::dependency::Dependency;
use crate::files::package_header::PackageHeader;
use crate::files::tick_range::TickRange;
use crate::files::version::Version;

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
