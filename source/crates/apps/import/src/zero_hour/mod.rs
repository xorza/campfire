use std::collections::BTreeMap;
use std::path::Path;

use bevy_math::UVec2;
use campfire_client_files::{
    CameraFile, ClientObjects, HeightGrid, MapConditions, MapLights, TerrainFile,
};
use campfire_codec::Toml;
use campfire_common::{MapName, PackageFingerprint};
use campfire_math::Num;
use campfire_package_disk::{PackageWriter, WriteError};
use campfire_store::DurableDir;
use campfire_values::PackagePath;

use crate::texture::dds_file::DdsFile;
use crate::texture::tga_file::{TgaAlpha, TgaFile, TgaHeader};
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::combat_import::{CombatImport, CombatSources, DroppedWeapon};
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::ini::armor_ini::ArmorIni;
use crate::zero_hour::format::ini::command_button_ini::CommandButtonIni;
use crate::zero_hour::format::ini::command_set_ini::CommandSetIni;
use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::multiplayer_ini::MultiplayerIni;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::player_template_ini::PlayerTemplateIni;
use crate::zero_hour::format::ini::terrain_ini::TerrainIni;
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::format::map::map_file::MapFile;
use crate::zero_hour::format::map::terrain::TextureClass;
use crate::zero_hour::game_data::GameData;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::install::{Install, InstallTexture, TextureKind};
use crate::zero_hour::look_writer::LookWriter;
use crate::zero_hour::map_import::{MapImport, MapSources};
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_writer::{ModelReport, ModelWriter};
use crate::zero_hour::production_import::{CommandSources, DroppedButton, ProductionImport};
use crate::zero_hour::rules_package::{ModeImport, RULES_FILES};
use crate::zero_hour::skirmish_start::SkirmishStart;
use crate::zero_hour::terrain_atlas::ClassTexture;
use crate::zero_hour::unit_types::TypeAdditions;

pub(crate) mod archive_path;
pub(crate) mod camera_import;
pub(crate) mod cliff_walls;
pub(crate) mod combat_import;
pub(crate) mod error;
pub(crate) mod format;
pub(crate) mod game_data;
pub(crate) mod game_path;
pub(crate) mod game_version;
pub(crate) mod import_name;
pub(crate) mod import_names;
pub(crate) mod imported_type;
pub(crate) mod install;
pub(crate) mod look_writer;
pub(crate) mod map_import;
pub(crate) mod model_assets;
pub(crate) mod model_import;
pub(crate) mod model_parts;
pub(crate) mod model_writer;
pub(crate) mod pose;
pub(crate) mod production_import;
pub(crate) mod rules_package;
pub(crate) mod skirmish_start;
pub(crate) mod terrain_atlas;
pub(crate) mod terrain_cells;
pub(crate) mod unit_types;

/// What an import wrote: the package's fingerprint, the models the objects name that the
/// package lacks, the textures its models name that no archive holds, which they draw
/// untextured, and each map it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: PackageFingerprint,
    /// The rules package's, beside it.
    pub rules: PackageFingerprint,
    pub skipped_models: Vec<SkippedModel>,
    pub missing_textures: Vec<String>,
    /// Each map the import wrote, by name.
    pub maps: BTreeMap<MapName, ImportedMap>,
    /// The command buttons the import does not convert, each with why (design 18, D3).
    pub dropped_buttons: Vec<DroppedButton>,
    /// The weapons the import does not convert, each with why (design 19, D6).
    pub dropped_weapons: Vec<DroppedWeapon>,
    /// The unit types' objects whose body the import gives no life (design 19, D5).
    pub unliving: Vec<String>,
    /// The unit types' objects whose body starts below its most life, which the engine starts
    /// full.
    pub wounded: Vec<String>,
}

/// What the import wrote of a map: its count of cells whose terrain draws a tile of no texture,
/// and the waypoints it dropped, past its bounds, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedMap {
    pub untextured_cells: usize,
    pub dropped_waypoints: Vec<String>,
}

/// A model the objects name that the package lacks, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedModel {
    pub name: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// No file of the install holds it.
    Missing,
    /// It is a particle emitter, which the importer does not convert.
    Emitter,
    /// It is an aggregate of a model's HLOD, a render object the game adds on a pivot, which
    /// the importer does not convert.
    Aggregate,
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
    /// the install's, its name its folder's, and every template its objects name a unit type of
    /// `data/units.toml`; each texture becomes a KTX2 file of `client/textures/`, at its path in
    /// the archives.
    pub fn write(&self, out: &Path) -> Result<Imported, ZeroHourError> {
        DurableDir::create_new(out).map_err(WriteError::CreateDir)?;
        let rules = Self::write_rules(&out.join(Self::RULES))?;
        let mut writer = PackageWriter::create(&out.join(Self::GAME))?;
        let terrain_types = self.read_inis("Terrain", TerrainIni::default(), TerrainIni::read)?;
        let game_data = self.read_inis("GameData", GameDataIni::default(), GameDataIni::read)?;
        let camera = camera_import::camera_file(&game_data).ok_or(ZeroHourError::Camera)?;
        let text = Toml::write(&camera);
        writer.write(CameraFile::path(), text.as_bytes())?;
        let ini = self.read_inis("Object", ObjectIni::default(), ObjectIni::read)?;
        let locomotors =
            self.read_inis("Locomotor", LocomotorIni::default(), LocomotorIni::read)?;
        let mut models = ModelWriter::new(ModelAssets::new(&self.install)?, &mut writer);
        let mut object_names = ImportNames::default();
        let looks = LookWriter {
            models: &mut models,
        }
        .units(&ini, &mut object_names)?;
        let ModelReport {
            skipped,
            missing_textures,
        } = models.report();
        let mut names = BTreeMap::<MapName, String>::new();
        let mut maps = BTreeMap::new();
        let mut class_sizes = BTreeMap::new();
        for map in self.install.maps() {
            let name = ImportName::of(map.folder.as_bytes()).map();
            if let Some(first) = names.insert(name.clone(), map.path.to_string()) {
                return Err(ZeroHourError::MapNameClash {
                    first,
                    second: map.path.to_string(),
                });
            }
            let bytes = self.install.read(&map.path)?;
            let in_map = |error| ZeroHourError::Map {
                map: map.path.to_string(),
                error,
            };
            let file = MapFile::read(&bytes).map_err(|error| in_map(error.into()))?;
            let textures = file
                .terrain
                .classes
                .iter()
                .map(|class| self.class_texture(class, &terrain_types, &mut class_sizes))
                .collect::<Result<Vec<_>, _>>()?;
            let sources = MapSources {
                ini: &ini,
                locomotors: &locomotors,
                names: &mut object_names,
                looks: &looks,
                lights: game_data.global_lights(),
                game: GameData::of(&game_data),
                textures: &textures,
            };
            let imported = MapImport::new(&file, sources).map_err(in_map)?;
            let map_data = fixed(&format!("map/{name}/map.toml"));
            writer.write(map_data, imported.map.as_bytes())?;
            writer.write(HeightGrid::path(&name), &imported.heights)?;
            writer.write(TerrainFile::path(&name), &imported.terrain)?;
            writer.write(MapConditions::path(&name), imported.conditions.as_bytes())?;
            writer.write(MapLights::path(&name), imported.lights.as_bytes())?;
            writer.write(ClientObjects::path(&name), imported.objects.as_bytes())?;
            maps.insert(
                name,
                ImportedMap {
                    untextured_cells: imported.untextured,
                    dropped_waypoints: imported.dropped_waypoints,
                },
            );
        }
        for InstallTexture { path, kind } in self.install.textures() {
            let bytes = self.install.read(&path)?;
            let texture = |error| ZeroHourError::Texture {
                path: path.to_string(),
                error,
            };
            let ktx2 = match kind {
                TextureKind::Dds => DdsFile::read(&bytes).map_err(texture)?.ktx2(),
                // Zero Hour's loader keeps a texel's alpha whatever the header counts.
                TextureKind::Tga => TgaFile::read(&bytes, TgaAlpha::Always)
                    .map_err(texture)?
                    .ktx2(),
            };
            writer.write(path.texture_path()?, &ktx2)?;
        }
        let templates = self.read_inis(
            "PlayerTemplate",
            PlayerTemplateIni::default(),
            PlayerTemplateIni::read,
        )?;
        let start = SkirmishStart::of(&templates, &ini, &locomotors, &mut object_names)?;
        let sets = self.read_inis("CommandSet", CommandSetIni::default(), CommandSetIni::read)?;
        let buttons = self.read_inis(
            "CommandButton",
            CommandButtonIni::default(),
            CommandButtonIni::read,
        )?;
        let multiplayer = self.read_inis(
            "Multiplayer",
            MultiplayerIni::default(),
            MultiplayerIni::read,
        )?;
        let money = multiplayer
            .default_money()
            .ok_or(ZeroHourError::DefaultMoney)?;
        let strings = self.string_table()?;
        let sources = CommandSources {
            ini: &ini,
            sets: &sets,
            buttons: &buttons,
            locomotors: &locomotors,
            strings: &strings,
        };
        let production = ProductionImport::of(sources, &mut object_names);
        // Every unit type is made: the maps', the start's and the command sets'. The stages
        // after read their closed set, and return what they give each.
        let types = object_names.close();
        let weapons = self.read_inis("Weapon", WeaponIni::default(), WeaponIni::read)?;
        let armors = self.read_inis("Armor", ArmorIni::default(), ArmorIni::read)?;
        let combat = CombatImport::of(
            CombatSources {
                ini: &ini,
                weapons: &weapons,
                armors: &armors,
            },
            &types,
        )?;
        let mode = ModeImport {
            start: &start,
            production: &production,
            combat: &combat,
            start_money: money,
        };
        writer.write(
            fixed("data/mode.toml"),
            rules_package::mode_data(mode).as_bytes(),
        )?;
        let text = rules_package::mode_text(&production.texts);
        writer.write(fixed("locale/en.ftl"), text.as_bytes())?;
        let units = types.units_file(TypeAdditions {
            weapon: &combat.weapons,
            command: &production.commands,
            production: &production.sections,
        });
        writer.write(fixed("data/units.toml"), units.text.as_bytes())?;
        // A mode whose types drive none caps no speed it moves at; 1 m a second is as good as any.
        let max_move_speed = units.max_speed.filter(|speed| *speed > Num::ZERO);
        let manifest = rules_package::mode_manifest(max_move_speed.unwrap_or(Num::ONE));
        writer.write(fixed("manifest.toml"), manifest.as_bytes())?;
        Ok(Imported {
            fingerprint: writer.finish()?,
            rules,
            skipped_models: skipped,
            missing_textures,
            maps,
            dropped_buttons: production.dropped,
            dropped_weapons: combat.dropped,
            unliving: combat.unliving,
            wounded: combat.wounded,
        })
    }
}

impl ZeroHour {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = "zero-hour";
    pub const GAME: &'static str = "game";

    /// Writes the rules package into the new directory `dir`; its fingerprint.
    fn write_rules(dir: &Path) -> Result<PackageFingerprint, ZeroHourError> {
        let mut writer = PackageWriter::create(dir)?;
        for (path, bytes) in RULES_FILES {
            writer.write(fixed(path), bytes)?;
        }
        Ok(writer.finish()?)
    }

    /// The install's string table, `Data\<language>\generals.csf`, as the game reads its
    /// language's.
    fn string_table(&self) -> Result<StringTable, ZeroHourError> {
        let language = self.install.language()?.ok_or(ZeroHourError::NoLanguage)?;
        let path = ArchivePath::of(&format!("data\\{language}\\generals.csf"));
        let bytes = self.install.read(&path)?;
        StringTable::read(&bytes).map_err(|error| ZeroHourError::StringTable {
            path: path.to_string(),
            error,
        })
    }

    /// `into` after `read` read each INI file of `name`, in the game's order.
    fn read_inis<T>(
        &self,
        name: &str,
        mut into: T,
        read: fn(&mut T, &[u8]) -> Result<(), IniError>,
    ) -> Result<T, ZeroHourError> {
        for path in self.install.inis(name) {
            let bytes = self.install.read(&path)?;
            read(&mut into, &bytes).map_err(|error| ZeroHourError::Ini {
                path: path.to_string(),
                error,
            })?;
        }
        Ok(into)
    }

    /// The texture of `class`: the TGA `Art\Terrain\` holds by the name its terrain type gives,
    /// as `readTexClass` opens it, its size read once into `sizes`; none for a class no type
    /// names, or a TGA the install lacks.
    fn class_texture(
        &self,
        class: &TextureClass,
        types: &TerrainIni,
        sizes: &mut BTreeMap<ArchivePath, UVec2>,
    ) -> Result<Option<ClassTexture>, ZeroHourError> {
        let Some(file) = types.texture(&class.name) else {
            return Ok(None);
        };
        let path = ArchivePath::of(&format!("art\\terrain\\{file}"));
        if path.extension() != Some("tga") || !self.install.contains(&path) {
            return Ok(None);
        }
        let size = if let Some(&size) = sizes.get(&path) {
            size
        } else {
            let bytes = self.install.read(&path)?;
            let header = TgaHeader::read(&bytes).map_err(|error| ZeroHourError::Texture {
                path: path.to_string(),
                error,
            })?;
            sizes.insert(path.clone(), header.size());
            header.size()
        };
        Ok(Some(ClassTexture {
            path: path.texture_path()?,
            size,
        }))
    }
}

/// The path `text` of a file the import writes by a name of its own.
fn fixed(text: &str) -> PackagePath {
    PackagePath::parse(text).expect("the import's own paths are package paths")
}

#[cfg(test)]
mod tests;
