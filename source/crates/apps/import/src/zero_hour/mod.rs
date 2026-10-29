use std::collections::BTreeMap;
use std::path::Path;

use campfire_common::{MapName, PackageFingerprint};
use campfire_package::ModeRead;
use campfire_package_disk::{PackageDir, PackageWriter, WriteError};
use campfire_package_format::FileIndex;
use campfire_store::DurableDir;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::game_write::GameWrite;
use crate::zero_hour::import_loss::ImportLoss;
use crate::zero_hour::install::Install;
use crate::zero_hour::road_pieces::RoadPieces;
use crate::zero_hour::rules_package::RULES_FILES;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

pub(crate) mod additions;
mod airfield_bones;
pub(crate) mod animation_drop_cause;
pub(crate) mod animation_import;
pub(crate) mod archive_path;
pub(crate) mod area_write;
pub(crate) mod art_paths;
pub(crate) mod axes;
pub(crate) mod barrel_recoil;
pub(crate) mod button_drop_cause;
pub(crate) mod camera_import;
pub(crate) mod chassis_import;
pub(crate) mod cliff_walls;
pub(crate) mod combat_import;
pub(crate) mod contain_drop_cause;
pub(crate) mod contain_import;
pub(crate) mod convex_clip;
pub(crate) mod draw_sets;
pub(crate) mod dropped;
pub(crate) mod dropped_roads;
pub(crate) mod effect_drop_cause;
pub(crate) mod effects_import;
pub(crate) mod error;
pub(crate) mod format;
pub(crate) mod game_path;
pub(crate) mod game_time;
pub(crate) mod game_version;
mod game_write;
pub(crate) mod import_loss;
pub(crate) mod import_name;
pub(crate) mod import_names;
pub(crate) mod import_team;
pub(crate) mod imported_type;
pub(crate) mod install;
pub(crate) mod look_flags;
pub(crate) mod map_import;
pub(crate) mod map_roads;
pub(crate) mod model_assets;
pub(crate) mod model_bones;
pub(crate) mod model_converter;
pub(crate) mod model_import;
pub(crate) mod model_nodes;
pub(crate) mod model_writer;
pub(crate) mod mover_import;
pub(crate) mod object_flight;
pub(crate) mod panel_drop_cause;
pub(crate) mod panel_import;
pub(crate) mod pivot_nodes;
mod portable_structure;
pub(crate) mod power_drop_cause;
mod press_import;
mod production_bones;
pub(crate) mod production_cx;
pub(crate) mod production_import;
mod propaganda_import;
pub(crate) mod road_buffer;
pub(crate) mod road_decals;
pub(crate) mod road_drop_cause;
pub(crate) mod road_mesh;
pub(crate) mod road_pieces;
pub(crate) mod rules_names;
pub(crate) mod rules_package;
pub(crate) mod science_import;
pub(crate) mod second_life_import;
pub(crate) mod skirmish_start;
pub(crate) mod sound_drop_cause;
pub(crate) mod sound_import;
pub(crate) mod state_drop_cause;
pub(crate) mod supply_drop_cause;
pub(crate) mod supply_import;
pub(crate) mod terrain_atlas;
pub(crate) mod terrain_cells;
pub(crate) mod type_name;
pub(crate) mod unit_types;
pub(crate) mod upgrade_drop_cause;
pub(crate) mod upgrade_import;
pub(crate) mod variation_drop_cause;
pub(crate) mod variation_import;
pub(crate) mod veterancy_import;
pub(crate) mod weapon_drop_cause;
pub(crate) mod wreck_import;
pub(crate) mod zero_hour_rules;

/// What an import wrote: the packages' fingerprints, each map, by name, which loads, the road
/// pieces each map draws, and what it does not hold of the install, in the order the import met
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: PackageFingerprint,
    /// The rules package's, beside it.
    pub rules: PackageFingerprint,
    pub maps: Vec<MapName>,
    pub roads: BTreeMap<MapName, RoadPieces>,
    pub losses: Vec<ImportLoss>,
}

/// A Zero Hour install, read as the game reads it and of a version the importer knows, which it
/// imports into a package.
#[derive(Debug)]
pub struct ZeroHour {
    install: Install,
    version: &'static GameVersion,
}

impl ZeroHour {
    /// The install under `root`, refused unless its archives are a version the importer knows.
    pub fn open(root: &Path) -> Result<Self, ZeroHourError> {
        Self::open_among(root, GameVersion::KNOWN)
    }

    /// The install under `root`, refused unless its archives are a version of `known`.
    fn open_among(root: &Path, known: &'static [GameVersion]) -> Result<Self, ZeroHourError> {
        let install = Install::open(root)?;
        let version = GameVersion::identify(&install.hashed()?, known)?;
        Ok(Self { install, version })
    }

    /// The version of the install.
    pub const fn version(&self) -> &'static GameVersion {
        self.version
    }

    /// Writes the new directory `out` of two packages: the rules package as the importer ships
    /// it, `zero-hour/`, and the install's, `game/`, a mode that depends on it; their
    /// fingerprints. Each map the game lists becomes `map/<name>/` and `client/maps/<name>/` of
    /// the install's, its name its directory's, and every template its objects name a unit type of
    /// `data/units.toml`; each texture becomes a KTX2 file of `client/textures/`, at its path in
    /// the archives.
    pub fn write(&self, out: &Path) -> Result<Imported, ZeroHourError> {
        DurableDir::create_new(out).map_err(WriteError::CreateDir)?;
        let rules = Self::write_rules(&out.join(Self::RULES))?;
        let game = ZeroHourRules::read(&self.install)?;
        let written = GameWrite::new(&self.install, &game, &out.join(Self::GAME))?.write()?;
        // The engine's own load of each map is the check of what the import wrote, so it
        // repeats none of the load's game.
        let package = PackageDir::new(out.join(Self::GAME));
        let load = |map: &MapName, error| ZeroHourError::Load {
            map: map.clone(),
            error: Box::new(error),
        };
        // A read that fails fails every map's load, so its error names the first.
        if let Some(first) = written.maps.first() {
            let read = ModeRead::at(&package).map_err(|error| load(first, error))?;
            for map in &written.maps {
                read.on(Some(map)).map_err(|error| load(map, error))?;
            }
        }
        Ok(Imported {
            fingerprint: written.fingerprint,
            rules,
            maps: written.maps,
            roads: written.roads,
            losses: written.losses,
        })
    }
}

impl ZeroHour {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = rules_package::RULES;
    pub const GAME: &'static str = "game";

    /// The version of what `write` writes from an install: raised with each change of its output,
    /// as a check keeps an import of the same install and version and writes nothing
    /// (09-zero-hour.md, A check keeps an unchanged import).
    pub const OUTPUT_VERSION: u32 = 92;

    /// Writes the rules package into the new directory `dir`; its fingerprint, which is
    /// `rules_fingerprint`'s.
    fn write_rules(dir: &Path) -> Result<PackageFingerprint, ZeroHourError> {
        let mut writer = PackageWriter::create(dir)?;
        for (path, bytes) in RULES_FILES {
            writer.write(fixed(path), bytes)?;
        }
        let written = writer.finish()?;
        debug_assert_eq!(
            written,
            Self::rules_fingerprint(),
            "the rules' index is its files'"
        );
        Ok(written)
    }

    /// The fingerprint of the rules package this importer writes, read from the index the
    /// workspace keeps of its files, so a kept import of other rules is known without a write
    /// (09-zero-hour.md, A check keeps an unchanged import).
    pub fn rules_fingerprint() -> PackageFingerprint {
        let index = include_bytes!("../../../../../packages/zero-hour/package.index");
        FileIndex::decode(index)
            .expect("the workspace's index of the rules decodes")
            .fingerprint()
    }
}

/// The path `text` of a file the import writes by a name of its own.
fn fixed(text: &str) -> PackagePath {
    PackagePath::parse(text).expect("the import's own paths are package paths")
}

#[cfg(test)]
mod tests;
