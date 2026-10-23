use std::collections::BTreeMap;
use std::path::Path;

use bevy_math::UVec2;
use campfire_client_files::{
    CameraFile, ClientFile, ClientObjects, ClientUnits, HudFile, KeysFile, MapConditions,
    MapLights, PickingFile, PixelRadius, ShownBars, TeamColors, TerrainFile,
};
use campfire_codec::Toml;
use campfire_common::{MapName, PackageFingerprint};
use campfire_math::Num;
use campfire_package::{MapFile, ModeFile, ModeRead, UnitsFile};
use campfire_package_disk::{PackageDir, PackageWriter, WriteError};
use campfire_package_format::PackageFiles;
use campfire_store::DurableDir;
use campfire_units::HeightGrid;
use campfire_values::{PackagePath, Speed};

use crate::texture::dds_file::DdsFile;
use crate::texture::tga_file::{TgaAlpha, TgaFile, TgaHeader};
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::terrain_ini::TerrainIni;
use crate::zero_hour::format::map::map_file::ZeroHourMap;
use crate::zero_hour::format::map::terrain::TextureClass;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_loss::ImportLoss;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::install::{Install, TextureKind};
use crate::zero_hour::map_import::{MapImport, MapSources};
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_writer::{ModelReport, ModelWriter};
use crate::zero_hour::production_import::ProductionImport;
use crate::zero_hour::rules_package::{ModeImport, RULES_FILES};
use crate::zero_hour::skirmish_start::SkirmishStart;
use crate::zero_hour::terrain_atlas::ClassTexture;
use crate::zero_hour::unit_types::TypeAdditions;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

pub(crate) mod archive_path;
pub(crate) mod axes;
pub(crate) mod camera_import;
pub(crate) mod cliff_walls;
pub(crate) mod combat_import;
pub(crate) mod dropped;
pub(crate) mod error;
pub(crate) mod format;
pub(crate) mod game_path;
pub(crate) mod game_time;
pub(crate) mod game_version;
pub(crate) mod import_loss;
pub(crate) mod import_name;
pub(crate) mod import_names;
pub(crate) mod import_team;
pub(crate) mod imported_type;
pub(crate) mod install;
pub(crate) mod map_import;
pub(crate) mod model_assets;
pub(crate) mod model_import;
pub(crate) mod model_nodes;
pub(crate) mod model_writer;
pub(crate) mod object_flight;
mod press_import;
pub(crate) mod production_import;
pub(crate) mod rules_names;
pub(crate) mod rules_package;
pub(crate) mod skirmish_start;
pub(crate) mod supply_import;
pub(crate) mod terrain_atlas;
pub(crate) mod terrain_cells;
pub(crate) mod type_name;
pub(crate) mod unit_types;
pub(crate) mod weapon_drop_cause;
pub(crate) mod zero_hour_rules;

/// What an import wrote: the packages' fingerprints, each map, by name, which loads, and what
/// it does not hold of the install, in the order the import met it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: PackageFingerprint,
    /// The rules package's, beside it.
    pub rules: PackageFingerprint,
    pub maps: Vec<MapName>,
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
        let mut writer = PackageWriter::create(&out.join(Self::GAME))?;
        let game = ZeroHourRules::read(&self.install)?;
        let camera = camera_import::camera_file(&game.game_data).ok_or(ZeroHourError::Camera)?;
        writer.write(CameraFile::path(), &camera.encode())?;
        let keys = press_import::keys_file(&game.mouse);
        writer.write(KeysFile::path(), &keys.encode())?;
        // The game draws a bar only over a unit selected or under the cursor
        // (`Drawable::drawHealthBar`).
        let hud = HudFile {
            bars: ShownBars::Selected,
        };
        writer.write(HudFile::path(), &hud.encode())?;
        // Units before structures, each widened to a radius of at least 10 pixels, where the game
        // picks the nearest triangle alone (09-zero-hour.md, Zero Hour's units come before its
        // structures).
        let picking = PickingFile {
            tiers: vec![game.names.no_structures.clone()],
            least_radius_px: PixelRadius::new(10.0).expect("a radius at least 0"),
        };
        writer.write(PickingFile::path(), &picking.encode())?;
        // The players' colors, each team's by its index (02-engine-core.md, Team colors in the
        // one material).
        let colors = TeamColors::new(game.multiplayer.colors().to_vec())?;
        writer.write(TeamColors::path(), &colors.encode())?;
        let mut models = ModelWriter::new(ModelAssets::new(&self.install)?, &mut writer);
        let mut object_names = ImportNames::default();
        let mut looks = models.units(&game.objects, &mut object_names)?;
        let ModelReport {
            skipped,
            missing_textures,
            textures: model_textures,
        } = models.report();
        let mut losses: Vec<ImportLoss> = skipped.into_iter().map(ImportLoss::Model).collect();
        losses.extend(missing_textures.into_iter().map(ImportLoss::Texture));
        let mut names = BTreeMap::<MapName, ArchivePath>::new();
        let mut maps = Vec::new();
        let mut class_textures = BTreeMap::new();
        for map in self.install.maps() {
            let name = ImportName::of(map.dir.as_bytes()).map()?;
            if let Some(first) = names.insert(name.clone(), map.path.clone()) {
                return Err(ZeroHourError::MapNameClash {
                    first,
                    second: map.path,
                });
            }
            let bytes = self.install.read(&map.path)?;
            let in_map = |error| ZeroHourError::Map {
                path: map.path.clone(),
                error,
            };
            let file = ZeroHourMap::read(&bytes).map_err(|error| in_map(error.into()))?;
            let textures = file
                .terrain
                .fields()
                .classes
                .iter()
                .map(|class| self.class_texture(class, &game.terrain, &mut class_textures))
                .collect::<Result<Vec<_>, _>>()?;
            let sources = MapSources {
                rules: &game,
                names: &mut object_names,
                looks: &looks,
                lights: game.game_data.global_lights(),
                game: game.game_data.terrain,
                textures: &textures,
            };
            let imported = MapImport::new(&file, sources).map_err(in_map)?;
            writer.write(MapFile::path(&name), imported.map.as_bytes())?;
            writer.write(HeightGrid::path(&name), &imported.heights)?;
            writer.write(TerrainFile::path(&name), &imported.terrain)?;
            writer.write(MapConditions::path(&name), &imported.conditions)?;
            writer.write(MapLights::path(&name), &imported.lights)?;
            writer.write(ClientObjects::path(&name), &imported.objects)?;
            if imported.untextured > 0 {
                losses.push(ImportLoss::UntexturedCells {
                    map: name.clone(),
                    cells: imported.untextured,
                });
            }
            let waypoints = imported.dropped_waypoints.into_iter();
            losses.extend(waypoints.map(|waypoint| ImportLoss::Waypoint {
                map: name.clone(),
                waypoint,
            }));
            maps.push(name);
        }
        // The textures the models and the terrain draw with, each converted once: no other file
        // of the install is drawn.
        let drawn = model_textures
            .into_iter()
            .chain(class_textures.keys().cloned());
        for (to, path) in texture_files(drawn)? {
            let kind =
                TextureKind::of(&path).expect("a model's and a terrain's textures are DDS or TGA");
            // A terrain's TGA was read for its size, and is not read again.
            let bytes = match class_textures.remove(&path) {
                Some(read) => read.bytes,
                None => self.install.read(&path)?,
            };
            let texture = |error| ZeroHourError::Texture {
                path: path.clone(),
                error,
            };
            let ktx2 = match kind {
                TextureKind::Dds => DdsFile::read(&bytes).map_err(texture)?.ktx2(),
                // Zero Hour's loader keeps a texel's alpha whatever the header counts.
                TextureKind::Tga => TgaFile::read(&bytes, TgaAlpha::Always)
                    .map_err(texture)?
                    .ktx2(),
            };
            writer.write(to, &ktx2)?;
        }
        let start = SkirmishStart::of(&game, &mut object_names)?;
        let money = game.multiplayer.money()?;
        let mut object_names = object_names.into_production();
        let production = ProductionImport::of(&game, &mut object_names);
        let mut object_names = object_names.into_projectiles();
        let combat = CombatImport::of(&game, &mut object_names)?;
        combat.add_projectile_looks(&mut looks);
        let text = Toml::write(&looks);
        writer.write(ClientUnits::path(), text.as_bytes())?;
        // Every unit type is made: the maps', the start's, the command sets' and the projectiles
        // their weapons fire. The stages after read their closed set, and return what they give
        // each.
        let types = object_names.close();
        let mode = ModeImport {
            start: &start,
            production: &production,
            combat: &combat,
            start_money: &money,
        };
        writer.write(ModeFile::path(), rules_package::mode_data(mode).as_bytes())?;
        let additions = TypeAdditions {
            weapon: &combat.weapons,
            command: &production.commands,
            production: &production.sections,
            nodes: &production.supply.nodes,
            drop_offs: &production.supply.drop_offs,
        };
        let units = types.units_file(additions, &game.names);
        writer.write(UnitsFile::path(), units.text.as_bytes())?;
        // A mode whose types drive none caps no speed it moves at; 1 m a second is as good as any.
        let max_move_speed = units
            .max_speed
            .and_then(Speed::new)
            .unwrap_or(Speed::new(Num::ONE).expect("1 m a second is a speed"));
        let manifest = rules_package::mode_manifest(max_move_speed);
        let text = rules_package::mode_text(&production.texts);
        writer.write(manifest.header.language.text_path(), text.as_bytes())?;
        let manifest = Toml::write(&manifest);
        writer.write(PackageFiles::manifest_path(), manifest.as_bytes())?;
        let fingerprint = writer.finish()?;
        // The engine's own load of each map is the check of what the import wrote, so it
        // repeats none of the load's game.
        let game = PackageDir::new(out.join(Self::GAME));
        let load = |map: &MapName, error| ZeroHourError::Load {
            map: map.clone(),
            error: Box::new(error),
        };
        // A read that fails fails every map's load, so its error names the first.
        if let Some(first) = maps.first() {
            let read = ModeRead::at(&game).map_err(|error| load(first, error))?;
            for map in &maps {
                read.on(Some(map)).map_err(|error| load(map, error))?;
            }
        }
        losses.extend(production.dropped.into_iter().map(ImportLoss::Button));
        let supply = production.supply.dropped.into_iter();
        losses.extend(supply.map(ImportLoss::Supply));
        losses.extend(combat.dropped.into_iter().map(ImportLoss::Weapon));
        losses.extend(combat.unliving.into_iter().map(ImportLoss::Unliving));
        losses.extend(combat.wounded.into_iter().map(ImportLoss::Wounded));
        Ok(Imported {
            fingerprint,
            rules,
            maps,
            losses,
        })
    }
}

impl ZeroHour {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = rules_package::RULES;
    pub const GAME: &'static str = "game";

    /// Writes the rules package into the new directory `dir`; its fingerprint.
    fn write_rules(dir: &Path) -> Result<PackageFingerprint, ZeroHourError> {
        let mut writer = PackageWriter::create(dir)?;
        for (path, bytes) in RULES_FILES {
            writer.write(fixed(path), bytes)?;
        }
        Ok(writer.finish()?)
    }

    /// The texture of `class`: the TGA `Art\Terrain\` holds by the name its terrain type gives,
    /// as `readTexClass` opens it, read once into `read` with its size, for its conversion; none
    /// for a class no type names, or a TGA the install lacks.
    fn class_texture(
        &self,
        class: &TextureClass,
        types: &TerrainIni,
        read: &mut BTreeMap<ArchivePath, TerrainTexture>,
    ) -> Result<Option<ClassTexture>, ZeroHourError> {
        let Some(file) = types.texture(&class.name) else {
            return Ok(None);
        };
        let path = ArchivePath::of(&format!("art\\terrain\\{file}"));
        if path.extension() != Some("tga") || !self.install.contains(&path) {
            return Ok(None);
        }
        let size = if let Some(texture) = read.get(&path) {
            texture.size
        } else {
            let bytes = self.install.read(&path)?;
            let header = TgaHeader::read(&bytes).map_err(|error| ZeroHourError::Texture {
                path: path.clone(),
                error,
            })?;
            let size = header.size();
            read.insert(path.clone(), TerrainTexture { size, bytes });
            size
        };
        Ok(Some(ClassTexture {
            path: game_path::texture(&path)?,
            size,
        }))
    }
}

/// A terrain's texture as the import read it for its size: the size, and the file's bytes, which
/// its conversion takes.
#[derive(Debug)]
struct TerrainTexture {
    size: UVec2,
    bytes: Vec<u8>,
}

/// Where each texture of `drawn` goes in the package, each once; an error for two that go to one
/// file, as an `x.dds` and an `x.tga` of one directory would.
fn texture_files(
    drawn: impl IntoIterator<Item = ArchivePath>,
) -> Result<BTreeMap<PackagePath, ArchivePath>, ZeroHourError> {
    let mut files = BTreeMap::<PackagePath, ArchivePath>::new();
    for path in drawn {
        let to = game_path::texture(&path)?;
        match files.get(&to) {
            Some(first) if *first == path => {}
            Some(first) => {
                return Err(ZeroHourError::TextureClash {
                    first: first.clone(),
                    second: path,
                });
            }
            None => {
                files.insert(to, path);
            }
        }
    }
    Ok(files)
}

/// The path `text` of a file the import writes by a name of its own.
fn fixed(text: &str) -> PackagePath {
    PackagePath::parse(text).expect("the import's own paths are package paths")
}

#[cfg(test)]
mod tests;
