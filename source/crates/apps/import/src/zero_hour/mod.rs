use std::collections::BTreeMap;
use std::path::Path;

use bevy_math::UVec2;
use campfire_client_files::{
    CameraFile, ClientModel, ClientObjects, ClientUnit, ClientUnits, LookVariant, MapConditions,
    MapLights, ModelScale, TerrainFile,
};
use campfire_codec::Toml;
use campfire_common::{Fingerprint, MapName};
use campfire_math::Num;
use campfire_package_disk::{PackageWriter, WriteError};
use campfire_store::DurableDir;
use campfire_values::PackagePath;

use crate::texture::dds_file::DdsFile;
use crate::texture::tga_file::{TgaFile, TgaHeader};
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::command_button_ini::CommandButtonIni;
use crate::zero_hour::command_set_ini::CommandSetIni;
use crate::zero_hour::error::{IniError, ZeroHourError};
use crate::zero_hour::game_data_ini::GameDataIni;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::install::{Install, InstallTexture, TextureKind};
use crate::zero_hour::locomotor_ini::LocomotorIni;
use crate::zero_hour::look_conditions::LookConditions;
use crate::zero_hour::map_file::MapFile;
use crate::zero_hour::map_import::{MapImport, MapSources};
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_writer::{ModelReport, ModelWriter, WrittenModel};
use crate::zero_hour::multiplayer_ini::MultiplayerIni;
use crate::zero_hour::object_ini::{DrawKind, DrawnState, ObjectIni, PlacedAs, StateLook};
use crate::zero_hour::player_template_ini::PlayerTemplateIni;
use crate::zero_hour::production_import::{CommandSources, DroppedButton, ProductionImport};
use crate::zero_hour::rules_package::RULES_FILES;
use crate::zero_hour::skirmish_start::SkirmishStart;
use crate::zero_hour::string_table::StringTable;
use crate::zero_hour::terrain::{ClassTexture, TextureClass};
use crate::zero_hour::terrain_ini::TerrainIni;

pub(crate) mod archive_path;
pub(crate) mod big_archive;
pub(crate) mod byte_cursor;
pub(crate) mod chunk_reader;
pub(crate) mod cliff_walls;
pub(crate) mod command_button_ini;
pub(crate) mod command_set_ini;
pub(crate) mod error;
pub(crate) mod game_data;
pub(crate) mod game_data_ini;
pub(crate) mod game_version;
pub(crate) mod hierarchy;
pub(crate) mod hlod;
pub(crate) mod import_name;
pub(crate) mod import_names;
pub(crate) mod imported_type;
pub(crate) mod ini_text;
pub(crate) mod install;
pub(crate) mod locomotor_ini;
pub(crate) mod look_conditions;
pub(crate) mod map_dict;
pub(crate) mod map_file;
pub(crate) mod map_import;
pub(crate) mod map_object;
pub(crate) mod model_assets;
pub(crate) mod model_import;
pub(crate) mod model_parts;
pub(crate) mod model_writer;
pub(crate) mod multiplayer_ini;
pub(crate) mod object_ini;
pub(crate) mod player_template_ini;
pub(crate) mod pose;
pub(crate) mod production_import;
pub(crate) mod ref_pack;
pub(crate) mod rules_package;
pub(crate) mod skirmish_start;
pub(crate) mod string_table;
pub(crate) mod terrain;
pub(crate) mod terrain_atlas;
pub(crate) mod terrain_cells;
pub(crate) mod terrain_ini;
pub(crate) mod w3d_chunks;
pub(crate) mod w3d_fields;
pub(crate) mod w3d_file;
pub(crate) mod w3d_mesh;

/// What an import wrote: the package's fingerprint, the models the objects name that the
/// package lacks, the textures its models name that no archive holds, which they draw
/// untextured, each map's count of cells whose terrain draws a tile of no texture, and the
/// waypoints each map dropped, past its bounds, by name, of the maps that dropped any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: Fingerprint,
    /// The rules package's, beside it.
    pub rules: Fingerprint,
    pub skipped_models: Vec<SkippedModel>,
    pub missing_textures: Vec<String>,
    pub untextured_cells: BTreeMap<MapName, usize>,
    pub dropped_waypoints: BTreeMap<MapName, Vec<String>>,
    /// The command buttons the import does not convert, each with why (design 18, D3).
    pub dropped_buttons: Vec<DroppedButton>,
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
}

/// A Zero Hour install, read as the game reads it and of a version the importer knows, which it
/// imports into a package.
#[derive(Debug)]
pub struct ZeroHour<'a> {
    install: Install,
    version: &'a GameVersion,
}

impl<'a> ZeroHour<'a> {
    /// The install under `root`, refused unless its archives are a version of `known`.
    pub fn open(root: &Path, known: &'a [GameVersion]) -> Result<ZeroHour<'a>, ZeroHourError> {
        let install = Install::open(root)?;
        let version = GameVersion::identify(&install.hashed()?, known)?;
        Ok(ZeroHour { install, version })
    }

    /// The version of the install.
    pub const fn version(&self) -> &'a GameVersion {
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
        let rules = ZeroHour::write_rules(&out.join(ZeroHour::RULES))?;
        let mut writer = PackageWriter::create(&out.join(ZeroHour::GAME))?;
        let mut write = |path: &str, bytes: &[u8]| {
            let path = PackagePath::parse(path).expect("the import's paths are package paths");
            writer.write(path, bytes).map_err(ZeroHourError::Write)
        };
        let terrain_types = self.read_inis("Terrain", TerrainIni::default(), TerrainIni::read)?;
        let game_data = self.read_inis("GameData", GameDataIni::default(), GameDataIni::read)?;
        let camera = game_data.camera_file().ok_or(ZeroHourError::Camera)?;
        let text = Toml::write(&camera);
        write(CameraFile::PATH, text.as_bytes())?;
        let ini = self.read_inis("Object", ObjectIni::default(), ObjectIni::read)?;
        let locomotors =
            self.read_inis("Locomotor", LocomotorIni::default(), LocomotorIni::read)?;
        let mut models = ModelWriter::new(ModelAssets::new(&self.install)?, &mut write);
        let mut object_names = ImportNames::default();
        let looks = ZeroHour::write_models(&ini, &mut models, &mut object_names)?;
        let ModelReport {
            skipped,
            missing_textures,
        } = models.report();
        let mut names = BTreeMap::<MapName, String>::new();
        let mut untextured_cells = BTreeMap::new();
        let mut dropped_waypoints = BTreeMap::new();
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
            let mut file = MapFile::read(&bytes).map_err(in_map)?;
            let terrain = &mut file.terrain;
            for class in terrain.classes.iter_mut().chain(&mut terrain.edge_classes) {
                class.texture = self.class_texture(class, &terrain_types, &mut class_sizes)?;
            }
            let sources = MapSources {
                ini: &ini,
                locomotors: &locomotors,
                names: &mut object_names,
                looks: &looks,
                lights: game_data.global_lights(),
                game: game_data.game_data(),
            };
            let imported = MapImport::new(&file, sources).map_err(in_map)?;
            untextured_cells.insert(name.clone(), imported.untextured);
            if !imported.dropped_waypoints.is_empty() {
                dropped_waypoints.insert(name.clone(), imported.dropped_waypoints);
            }
            write(&format!("map/{name}/map.toml"), imported.map.as_bytes())?;
            write(&format!("map/{name}/heights.bin"), &imported.heights)?;
            write(&TerrainFile::path(name.as_str()), &imported.terrain)?;
            write(
                &MapConditions::path(name.as_str()),
                imported.conditions.as_bytes(),
            )?;
            write(&MapLights::path(name.as_str()), imported.lights.as_bytes())?;
            write(
                &ClientObjects::path(name.as_str()),
                imported.objects.as_bytes(),
            )?;
        }
        for InstallTexture { path, kind } in self.install.textures() {
            let bytes = self.install.read(&path)?;
            let texture = |error| ZeroHourError::Texture {
                path: path.to_string(),
                error,
            };
            let ktx2 = match kind {
                TextureKind::Dds => DdsFile::read(&bytes).map_err(texture)?.ktx2(),
                TextureKind::Tga => TgaFile::read(&bytes).map_err(texture)?.ktx2(),
            };
            write(path.texture_path().as_str(), &ktx2)?;
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
        write(
            "data/mode.toml",
            rules_package::mode_data(&start, &production, money).as_bytes(),
        )?;
        let text = rules_package::mode_text(&production.texts);
        write("locale/en.ftl", text.as_bytes())?;
        let units = object_names.units_file();
        write("data/units.toml", units.text.as_bytes())?;
        // A mode whose types drive none caps no speed it moves at; 1 m a second is as good as any.
        let max_move_speed = units.max_speed.filter(|speed| *speed > Num::ZERO);
        let manifest = rules_package::mode_manifest(max_move_speed.unwrap_or(Num::ONE));
        write("manifest.toml", manifest.as_bytes())?;
        Ok(Imported {
            fingerprint: writer.finish()?,
            rules,
            skipped_models: skipped,
            missing_textures,
            untextured_cells,
            dropped_waypoints,
            dropped_buttons: production.dropped,
        })
    }

    /// Writes `client/units.toml` of `ini`'s objects, through `models`, and gives it: each
    /// object's look, its models, each at its object's asset scale, with the nodes its default
    /// look hides, a look of no models for an object whose draw modules name none, as it draws
    /// nothing, and no look for one whose models the package lacks, which the client shows as
    /// the engine's shape; and the look of each object that maps place as client objects, as the client's
    /// tree or prop buffer draws it (design 12, D14): a tree's models of its tree draw modules, a
    /// prop's of its first draw module, when the `W3DModelDraw` family's, each created by its
    /// name, so only the meshes hidden by their own flag are hidden.
    fn write_models<W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>>(
        ini: &ObjectIni,
        models: &mut ModelWriter<'_, W>,
        names: &mut ImportNames,
    ) -> Result<ClientUnits, ZeroHourError> {
        let mut units = ClientUnits::default();
        let shown = StateLook::default();
        for object in ini.objects() {
            let scale = object.scale();
            let draws = object.draws();
            // A unit draws each module's best state, as its drawable does.
            let unit = |conditions| draws.iter().map(move |draw| draw.drawn(conditions));
            let Some(unit_look) = ZeroHour::look(unit, scale, models)? else {
                continue;
            };
            let unit_type = names.take(object)?;
            // A tree draws its module's model whatever the map; a prop the model of its first
            // module's best state, all of it shown, as `W3DTerrainVisual::addProp` draws it.
            let unhidden = |model| DrawnState {
                model,
                look: &shown,
            };
            let drawn = |conditions| match object.placed_as() {
                PlacedAs::Bridge | PlacedAs::Unit => Vec::new(),
                PlacedAs::Tree => draws
                    .iter()
                    .filter(|draw| draw.kind == DrawKind::Tree)
                    .map(|draw| unhidden(draw.model.as_deref()))
                    .collect(),
                PlacedAs::Prop => draws
                    .first()
                    .filter(|draw| draw.kind == DrawKind::Family)
                    .map(|draw| unhidden(draw.drawn(conditions).model))
                    .into_iter()
                    .collect(),
            };
            if let Some(object_look) = ZeroHour::look(drawn, scale, models)?
                && !object_look.models.is_empty()
            {
                units.objects.insert(unit_type.declared(), object_look);
            }
            units.units.insert(unit_type.declared(), unit_look);
        }
        let text = Toml::write(&units);
        models.write("client/units.toml", text.as_bytes())?;
        Ok(units)
    }

    /// The look of what `drawn` draws on a map of each conditions, at `scale`: its models on a
    /// map of none, and, when a night map, a snowy map or both draw other models, a variant for
    /// each of the three, so the client's best match finds each one's own. A look that draws
    /// nothing is empty when no model it names draws anything, and none when the package lacks
    /// one, so the client shows its loss.
    fn look<'d, I, W>(
        drawn: impl Fn(LookConditions) -> I,
        scale: ModelScale,
        models: &mut ModelWriter<'_, W>,
    ) -> Result<Option<ClientUnit>, ZeroHourError>
    where
        I: IntoIterator<Item = DrawnState<'d>>,
        W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>,
    {
        let own = ZeroHour::models(drawn(LookConditions::default()), scale, models)?;
        let mut skipped = own.skipped;
        let mut variants = Vec::new();
        for conditions in LookConditions::VARIANTS {
            let looked = ZeroHour::models(drawn(conditions), scale, models)?;
            skipped |= looked.skipped;
            variants.push(LookVariant {
                conditions: conditions.names(),
                models: looked.models,
            });
        }
        if variants.iter().all(|variant| variant.models == own.models) {
            variants.clear();
        }
        if skipped && own.models.is_empty() && variants.is_empty() {
            return Ok(None);
        }
        Ok(Some(ClientUnit {
            models: own.models,
            variants,
        }))
    }

    /// The models of `drawn`, each with the look whose hidden nodes it hides, at `scale`, of those
    /// that draw one the package holds, and whether the package lacks one it names.
    fn models<'d, W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>>(
        drawn: impl IntoIterator<Item = DrawnState<'d>>,
        scale: ModelScale,
        models: &mut ModelWriter<'_, W>,
    ) -> Result<LookModels, ZeroHourError> {
        let mut looked = LookModels {
            models: Vec::new(),
            skipped: false,
        };
        for DrawnState { model, look } in drawn {
            let Some(model) = model else {
                continue;
            };
            let name = model.to_ascii_lowercase();
            match models.model(&name)? {
                WrittenModel::Parts(parts) => looked.models.push(ClientModel {
                    model: model_writer::path(&name),
                    scale,
                    hide: parts.hidden(look),
                }),
                WrittenModel::Nothing => {}
                WrittenModel::Skipped => looked.skipped = true,
            }
        }
        Ok(looked)
    }
}

/// The models a look draws on a map of some conditions, and whether the package lacks a model
/// it names.
#[derive(Debug)]
struct LookModels {
    models: Vec<ClientModel>,
    skipped: bool,
}

impl ZeroHour<'_> {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = "zero-hour";
    pub const GAME: &'static str = "game";

    /// Writes the rules package into the new directory `dir`; its fingerprint.
    fn write_rules(dir: &Path) -> Result<Fingerprint, ZeroHourError> {
        let mut writer = PackageWriter::create(dir)?;
        for (path, bytes) in RULES_FILES {
            let path =
                PackagePath::parse(path).expect("the rules package's paths are package paths");
            writer.write(path, bytes)?;
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
            path: path.texture_path(),
            size,
        }))
    }
}

#[cfg(test)]
mod tests;
