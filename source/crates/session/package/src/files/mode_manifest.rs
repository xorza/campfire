use std::collections::BTreeMap;

use campfire_capabilities::{CapabilityError, Release};
use campfire_common::Version;
use campfire_declare::CapabilitySet;
use campfire_mode::TeamManifest;
use campfire_script::{ApiVersion, ScriptLimits};
use campfire_sim::Capability;
use campfire_values::Speed;
use serde::{Deserialize, Deserializer};

use crate::files::dependency::Dependency;
use crate::files::package_header::PackageHeader;
use crate::files::package_name::PackageName;
use crate::files::tick_range::TickRange;
use crate::language::Language;

/// A mode's manifest: its header's fields, then the rules its matches run by, in one table. Its
/// capabilities read as the list the file names, `C` a `Vec`, which `checked` makes a set the
/// release builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeManifest<C = CapabilitySet> {
    pub header: PackageHeader,
    /// The capabilities its matches use; the release installs only these.
    pub capabilities: C,
    pub tick_hz: TickRange,
    /// The playing teams, in order; their seats in that order make the player seats.
    pub teams: Vec<TeamManifest>,
    /// The most any unit walks.
    pub max_move_speed: Speed,
    pub script_limits: ScriptLimits,
    /// By name; in the workspace each is a path, relative to the manifest.
    pub dependencies: BTreeMap<PackageName, Dependency>,
}

impl ModeManifest<Vec<Capability>> {
    /// The manifest with its capabilities as their set; an error when they make none, as
    /// `Release::capability_set` says.
    pub(crate) fn checked(self) -> Result<ModeManifest, CapabilityError> {
        Ok(ModeManifest {
            header: self.header,
            capabilities: Release::capability_set(&self.capabilities)?,
            tick_hz: self.tick_hz,
            teams: self.teams,
            max_move_speed: self.max_move_speed,
            script_limits: self.script_limits,
            dependencies: self.dependencies,
        })
    }
}

impl ModeManifest {
    /// How many player seats its teams have together.
    pub fn seats(&self) -> u64 {
        self.teams.iter().map(|team| u64::from(team.seats)).sum()
    }
}

/// The flat table of a mode's manifest, its header's fields among the others.
impl<'de> Deserialize<'de> for ModeManifest<Vec<Capability>> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            name: PackageName,
            version: Version,
            api: ApiVersion,
            language: Language,
            capabilities: Vec<Capability>,
            tick_hz: TickRange,
            teams: Vec<TeamManifest>,
            max_move_speed: Speed,
            script_limits: ScriptLimits,
            #[serde(default)]
            dependencies: BTreeMap<PackageName, Dependency>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(Self {
            header: PackageHeader {
                name: fields.name,
                version: fields.version,
                api: fields.api,
                language: fields.language,
            },
            capabilities: fields.capabilities,
            tick_hz: fields.tick_hz,
            teams: fields.teams,
            max_move_speed: fields.max_move_speed,
            script_limits: fields.script_limits,
            dependencies: fields.dependencies,
        })
    }
}
