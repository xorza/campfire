use std::collections::BTreeMap;

use campfire_capabilities::{CapabilityError, Release};
use campfire_common::Version;
use campfire_declarations::CapabilitySet;
use campfire_mode::TeamManifest;
use campfire_script::{ApiVersion, ScriptLimits};
use campfire_sim::Capability;
use campfire_values::PackageName;
use campfire_values::{Speed, Team};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::files::dependency::Dependency;
use crate::files::package_header::PackageHeader;
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
    /// The playing teams, in order; their player slots in that order make the player slots.
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
    /// How many player slots its teams have together.
    pub fn player_slots(&self) -> u64 {
        self.teams
            .iter()
            .map(|team| u64::from(team.player_slots))
            .sum()
    }

    /// The team of `name`, by its place among the teams; none for a name no team has.
    pub fn team_named(&self, name: &str) -> Option<Team> {
        let at = self.teams.iter().position(|team| team.name == *name)?;
        Some(Team::new(u8::try_from(at).expect("a team's place fits u8")))
    }
}

/// The flat table of a mode's manifest, its header's fields among the others, as its file holds
/// it, which reads and writes the same fields: `kind`, which `Manifest`'s tag reads before the
/// fields, is written alone (03-game-scripting.md, The manifest writes as it reads).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    #[serde(skip_deserializing)]
    kind: Kind,
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

/// The kind of package a mode's manifest names.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    #[default]
    Mode,
}

impl Serialize for ModeManifest<Vec<Capability>> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Self {
            header,
            capabilities,
            tick_hz,
            teams,
            max_move_speed,
            script_limits,
            dependencies,
        } = self.clone();
        Fields {
            kind: Kind::Mode,
            name: header.name,
            version: header.version,
            api: header.api,
            language: header.language,
            capabilities,
            tick_hz,
            teams,
            max_move_speed,
            script_limits,
            dependencies,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModeManifest<Vec<Capability>> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;
    use crate::files::manifest::Manifest;

    /// The lane test mode's manifest, which names a dependency.
    const LANE: &str = include_str!("../../../../../packages/test/engine/modes/lane/manifest.toml");

    fn mode(text: &str) -> ModeManifest<Vec<Capability>> {
        let Manifest::Mode(manifest) = Toml::parse(text).unwrap() else {
            panic!("a mode's manifest");
        };
        manifest
    }

    /// A manifest read from its file writes its kind first, then the fields it read, and reads
    /// back to itself: one shape both ways.
    #[test]
    fn a_mode_manifest_writes_what_it_reads() {
        let read = mode(LANE);
        let written = Toml::write(&read);
        assert!(
            written.starts_with("kind = \"mode\"\nname = \"test-lane\"\n"),
            "{written}"
        );
        assert_eq!(mode(&written), read);
        assert_eq!(read.dependencies.len(), 3);
    }
}
