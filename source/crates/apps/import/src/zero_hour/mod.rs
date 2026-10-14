use std::collections::BTreeMap;
use std::path::Path;

use bevy_math::UVec2;
use campfire_codec::Toml;
use campfire_common::{Fingerprint, MapName};
use campfire_package::{
    CameraFile, ClientModel, ClientObjects, ClientUnit, ClientUnits, MapLights, ModelScale,
    PackageWriter, TerrainFile, WriteError,
};
use campfire_store::DurableFile;
use campfire_values::PackagePath;

use crate::texture::dds_file::DdsFile;
use crate::texture::tga_file::TgaFile;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::{IniError, ZeroHourError};
use crate::zero_hour::game_data_ini::GameDataIni;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::install::{Install, InstallTexture, TextureKind};
use crate::zero_hour::map_file::MapFile;
use crate::zero_hour::map_import::{MapImport, MapSources};
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_writer::{ModelReport, ModelWriter};
use crate::zero_hour::object_ini::{DefaultLook, DrawKind, DrawModule, ObjectIni, PlacedAs};
use crate::zero_hour::rules_package::{MODE_DATA, MODE_MANIFEST, MODE_TEXT, RULES_FILES};
use crate::zero_hour::terrain::{ClassTexture, TextureClass};
use crate::zero_hour::terrain_ini::TerrainIni;
use crate::zero_hour::unit_types::UnitTypes;

pub(crate) mod archive_path;
pub(crate) mod big_archive;
pub(crate) mod chunk_reader;
pub(crate) mod error;
pub(crate) mod game_data;
pub(crate) mod game_data_ini;
pub(crate) mod game_version;
pub(crate) mod hierarchy;
pub(crate) mod hlod;
pub(crate) mod import_name;
pub(crate) mod ini_text;
pub(crate) mod install;
pub(crate) mod map_file;
pub(crate) mod map_import;
pub(crate) mod map_object;
pub(crate) mod model_assets;
pub(crate) mod model_import;
pub(crate) mod model_parts;
pub(crate) mod model_writer;
pub(crate) mod object_ini;
pub(crate) mod pose;
pub(crate) mod ref_pack;
pub(crate) mod rules_package;
pub(crate) mod terrain;
pub(crate) mod terrain_atlas;
pub(crate) mod terrain_cells;
pub(crate) mod terrain_ini;
pub(crate) mod unit_types;
pub(crate) mod w3d_chunks;
pub(crate) mod w3d_fields;
pub(crate) mod w3d_file;
pub(crate) mod w3d_mesh;

/// What an import wrote: the package's fingerprint, the models the objects name that the
/// package lacks, the textures its models name that no archive holds, which they draw
/// untextured, and each map's count of cells whose terrain draws a tile of no texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: Fingerprint,
    /// The rules package's, beside it.
    pub rules: Fingerprint,
    pub skipped_models: Vec<SkippedModel>,
    pub missing_textures: Vec<String>,
    pub untextured_cells: BTreeMap<MapName, usize>,
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
        DurableFile::create_new_dir(out)
            .map_err(WriteError::CreateDir)
            .map_err(ZeroHourError::Write)?;
        let rules = ZeroHour::write_rules(&out.join(ZeroHour::RULES))?;
        let mut writer =
            PackageWriter::create(&out.join(ZeroHour::GAME)).map_err(ZeroHourError::Write)?;
        let mut write = |path: &str, bytes: &[u8]| {
            let path = PackagePath::parse(path).expect("the import's paths are package paths");
            writer.write(path, bytes).map_err(ZeroHourError::Write)
        };
        write("manifest.toml", MODE_MANIFEST.as_bytes())?;
        write("data/mode.toml", MODE_DATA.as_bytes())?;
        write("locale/en.ftl", MODE_TEXT.as_bytes())?;
        let terrain_types = self.read_inis("Terrain", TerrainIni::default(), TerrainIni::read)?;
        let game_data = self.read_inis("GameData", GameDataIni::default(), GameDataIni::read)?;
        let camera = game_data.camera_file().ok_or(ZeroHourError::Camera)?;
        let text = Toml::write(&camera);
        write(CameraFile::PATH, text.as_bytes())?;
        let ini = self.read_inis("Object", ObjectIni::default(), ObjectIni::read)?;
        let mut models = ModelWriter::new(ModelAssets::new(&self.install)?, &mut write);
        let looks = ZeroHour::write_models(&ini, &mut models)?;
        let ModelReport {
            skipped,
            missing_textures,
        } = models.report();
        let mut unit_types = UnitTypes::default();
        let mut names = BTreeMap::<MapName, String>::new();
        let mut untextured_cells = BTreeMap::new();
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
                types: &mut unit_types,
                looks: &looks,
                lights: game_data.global_lights(),
                game: game_data.game_data(),
            };
            let imported = MapImport::new(&file, sources).map_err(in_map)?;
            untextured_cells.insert(name.clone(), imported.untextured);
            write(&format!("map/{name}/map.toml"), imported.map.as_bytes())?;
            write(&format!("map/{name}/heights.bin"), &imported.heights)?;
            write(&TerrainFile::path(name.as_str()), &imported.terrain)?;
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
        write("data/units.toml", unit_types.toml().as_bytes())?;
        Ok(Imported {
            fingerprint: writer.finish().map_err(ZeroHourError::Write)?,
            rules,
            skipped_models: skipped,
            missing_textures,
            untextured_cells,
        })
    }

    /// Writes `client/units.toml` of `ini`'s objects, through `models`, and gives it: each
    /// object's look, its models, each at its object's asset scale, with the nodes its default
    /// look hides; and the look of each object that maps place as client objects, as the client's
    /// tree or prop buffer draws it (design 12, D14): a tree's models of its tree draw modules, a
    /// prop's of its first draw module, when the `W3DModelDraw` family's, each created by its
    /// name, so only the meshes hidden by their own flag are hidden.
    fn write_models<W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>>(
        ini: &ObjectIni,
        models: &mut ModelWriter<'_, W>,
    ) -> Result<ClientUnits, ZeroHourError> {
        let mut units = ClientUnits::default();
        let mut types = BTreeMap::<String, String>::new();
        let unhidden = DefaultLook::default();
        for object in ini.objects() {
            let scale = object.scale();
            let draws = object.draws();
            let entries =
                ZeroHour::models(draws.iter().map(|draw| (draw, &draw.look)), scale, models)?;
            if entries.is_empty() {
                continue;
            }
            let unit_type = object.import_name();
            if let Some(first) = types.insert(unit_type.as_str().to_owned(), object.name.clone()) {
                return Err(ZeroHourError::UnitTypeClash {
                    first,
                    second: object.name.clone(),
                });
            }
            let drawn: Vec<&DrawModule> = match object.placed_as() {
                PlacedAs::Bridge | PlacedAs::Unit => Vec::new(),
                PlacedAs::Tree => draws
                    .iter()
                    .filter(|draw| draw.kind == DrawKind::Tree)
                    .collect(),
                PlacedAs::Prop => draws
                    .first()
                    .filter(|draw| draw.kind == DrawKind::Family)
                    .into_iter()
                    .collect(),
            };
            let drawn = drawn.into_iter().map(|draw| (draw, &unhidden));
            let look = ZeroHour::models(drawn, scale, models)?;
            if !look.is_empty() {
                units
                    .objects
                    .insert(unit_type.declared(), ClientUnit { models: look });
            }
            units
                .units
                .insert(unit_type.declared(), ClientUnit { models: entries });
        }
        let text = Toml::write(&units);
        models.write("client/units.toml", text.as_bytes())?;
        Ok(units)
    }

    /// The models of `draws`, each with the look whose hidden nodes it hides, at `scale`, of those
    /// that draw one the package holds.
    fn models<'d, W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>>(
        draws: impl IntoIterator<Item = (&'d DrawModule, &'d DefaultLook)>,
        scale: ModelScale,
        models: &mut ModelWriter<'_, W>,
    ) -> Result<Vec<ClientModel>, ZeroHourError> {
        let mut entries = Vec::new();
        for (draw, look) in draws {
            let Some(model) = &draw.model else {
                continue;
            };
            let name = model.to_ascii_lowercase();
            if let Some(parts) = models.parts(&name)? {
                entries.push(ClientModel {
                    model: model_writer::path(&name),
                    scale,
                    hide: parts.hidden(look),
                });
            }
        }
        Ok(entries)
    }
}

impl ZeroHour<'_> {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = "zero-hour";
    pub const GAME: &'static str = "game";

    /// Writes the rules package into the new directory `dir`; its fingerprint.
    fn write_rules(dir: &Path) -> Result<Fingerprint, ZeroHourError> {
        let mut writer = PackageWriter::create(dir).map_err(ZeroHourError::Write)?;
        for (path, bytes) in RULES_FILES {
            let path =
                PackagePath::parse(path).expect("the rules package's paths are package paths");
            writer.write(path, bytes).map_err(ZeroHourError::Write)?;
        }
        writer.finish().map_err(ZeroHourError::Write)
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
            let tga = TgaFile::read(&bytes).map_err(|error| ZeroHourError::Texture {
                path: path.to_string(),
                error,
            })?;
            sizes.insert(path.clone(), tga.size());
            tga.size()
        };
        Ok(Some(ClassTexture {
            path: path.texture_path(),
            size,
        }))
    }
}

#[cfg(test)]
mod tests {
    use campfire_package::{ModePackages, PackageDir, PackageFiles};
    use campfire_store::internals::Scratch;
    use campfire_values::DeclaredName;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::texture::internals::ktx2;
    use crate::zero_hour::big_archive::internals::big;
    use crate::zero_hour::error::VersionDifference;
    use crate::zero_hour::game_data::GameData;
    use crate::zero_hour::game_version::ArchiveHash;
    use crate::zero_hour::install::internals::{fixture, packed_map};
    use crate::zero_hour::map_file::internals::map;
    use crate::zero_hour::model_import::{Converted, ModelImport};
    use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};

    /// The fixture install's version, as its archives hold their bytes now.
    fn version_of(scratch: &Scratch) -> [GameVersion; 1] {
        let archives =
            ["A.big", "b.big", "c.BIG", "ZH_Generals/base.big"].map(|path| ArchiveHash {
                path,
                sha256: Sha256::digest(scratch.read(format!("zh/{path}"))).into(),
            });
        [GameVersion {
            name: "fixture",
            archives: Box::leak(Box::new(archives)),
        }]
    }

    #[test]
    fn an_install_of_a_known_version_imports_its_maps_and_another_does_not() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let known = version_of(&scratch);
        let zero_hour = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        assert_eq!(zero_hour.version().name, "fixture");

        // The fixture map, in its folder `Fixture Map`, is `fixture_map`: its files are the ones
        // its import gives, and its two unit types the package's.
        let imported = zero_hour.write(&scratch.path("one")).unwrap();
        let fingerprint = imported.fingerprint;
        // Its terrain's sand of 2 × 1 texels holds no tile, and its grass has no texture, so each
        // of the 3 × 2 cells the game draws is untextured.
        let named = MapName::new("fixture_map").unwrap();
        assert_eq!(imported.untextured_cells, [(named.clone(), 6)].into());
        // Beside it, the rules package as the workspace holds it, which the imported mode, of
        // its manifest and mode data, depends on and loads with.
        assert_eq!(scratch.names("one"), ["game", "zero-hour"]);
        assert_eq!(
            scratch.names("one/game"),
            [
                "client",
                "data",
                "locale",
                "manifest.toml",
                "map",
                "package.index"
            ]
        );
        assert_eq!(
            scratch.read("one/zero-hour/package.index"),
            include_bytes!("../../../../../packages/zero-hour/package.index")
        );
        let mode = ModePackages::from_dir(&scratch.path("one/game"), &named);
        assert!(mode.is_ok(), "{mode:?}");
        assert_eq!(scratch.names("one/game/map"), ["fixture_map"]);
        assert_eq!(
            scratch.names("one/game/map/fixture_map"),
            ["heights.bin", "map.toml"]
        );
        assert_eq!(
            scratch.names("one/game/client/maps/fixture_map"),
            ["lights.toml", "objects.toml", "terrain.bin"]
        );
        // The class whose type's texture the install holds names it, the sign's TGA of 2 × 1
        // texels; the other names none.
        let mut file = MapFile::read(&map(8)).unwrap();
        file.terrain.classes[0].texture = Some(ClassTexture {
            path: PackagePath::parse("client/textures/art/terrain/tsand.ktx2").unwrap(),
            size: UVec2::new(2, 1),
        });
        let mut ini = ObjectIni::default();
        for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
            ini.read(objects.as_bytes()).unwrap();
        }
        let looks = Toml::parse(&scratch.read_text("one/game/client/units.toml")).unwrap();
        let mut types = UnitTypes::default();
        let sources = MapSources {
            ini: &ini,
            types: &mut types,
            looks: &looks,
            lights: 3,
            game: GameData {
                adjust_cliff_textures: true,
                three_way_blends: false,
            },
        };
        let expected = MapImport::new(&file, sources).unwrap();
        assert_eq!(
            scratch.read_text("one/game/map/fixture_map/map.toml"),
            expected.map
        );
        assert_eq!(
            scratch.read("one/game/map/fixture_map/heights.bin"),
            expected.heights
        );
        assert_eq!(
            scratch.read("one/game/client/maps/fixture_map/terrain.bin"),
            expected.terrain
        );
        assert_eq!(
            scratch.read_text("one/game/client/maps/fixture_map/lights.toml"),
            expected.lights
        );
        assert_eq!(
            scratch.read_text("one/game/client/maps/fixture_map/objects.toml"),
            expected.objects
        );
        assert_eq!(scratch.read_text("one/game/data/units.toml"), types.toml());
        let camera = Toml::parse::<CameraFile>(&scratch.read_text("one/game/client/camera.toml"));
        assert_eq!(camera.unwrap().field_of_view, 50.0);
        // Each texture once, at its path in the archives, as its conversion gives it.
        let [rock, sign] = ktx2();
        assert_eq!(
            scratch.names("one/game/client/textures/art/textures"),
            ["rock.ktx2", "sign.ktx2"]
        );
        assert_eq!(
            scratch.read("one/game/client/textures/art/textures/rock.ktx2"),
            rock
        );
        assert_eq!(
            scratch.read("one/game/client/textures/art/textures/sign.ktx2"),
            sign
        );
        assert_eq!(
            scratch.read("one/game/client/textures/art/terrain/tsand.ktx2"),
            sign
        );
        // Two imports write the same bytes, and the package reads as the fingerprint names it.
        assert_eq!(
            zero_hour.write(&scratch.path("two")).unwrap().fingerprint,
            fingerprint
        );
        assert_eq!(
            scratch.read("two/game/package.index"),
            scratch.read("one/game/package.index")
        );
        assert_eq!(
            PackageFiles::read(&PackageDir::new(scratch.path("one/game")))
                .unwrap()
                .fingerprint(),
            fingerprint
        );
    }

    #[test]
    fn an_install_of_two_maps_of_one_name_or_of_an_unknown_version_does_not_import() {
        let scratch = Scratch::new();
        fixture(&scratch);
        // A second folder whose name gives `fixture_map` is refused.
        scratch.write(
            "zh/c.BIG",
            big(&[
                ("c.ini", b"c"),
                ("Maps\\Fixture_Map\\Fixture_Map.map", &packed_map()),
            ]),
        );
        let known = version_of(&scratch);
        let clash = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        assert!(matches!(
            clash.write(&scratch.path("three")),
            Err(ZeroHourError::MapNameClash { first, second })
                if first == "maps\\fixture map\\fixture map.map"
                    && second == "maps\\fixture_map\\fixture_map.map"
        ));

        // An archive of other bytes is no version the importer knows.
        scratch.write("zh/c.BIG", b"BIGF");
        assert!(matches!(
            ZeroHour::open(&scratch.path("zh"), &known),
            Err(ZeroHourError::Archive { .. })
        ));
        scratch.write("zh/c.BIG", big(&[("c.ini", b"C")]));
        assert!(matches!(
            ZeroHour::open(&scratch.path("zh"), &known),
            Err(ZeroHourError::UnknownVersion {
                archive,
                difference: VersionDifference::OtherBytes,
            }) if archive == "c.big"
        ));
    }

    #[test]
    fn an_import_writes_each_model_once_and_each_object_s_models() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let known = version_of(&scratch);
        let zero_hour = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        let imported = zero_hour.write(&scratch.path("one")).unwrap();
        // Each model the objects name once, as its conversion gives it; its material file; and
        // each object's models with the nodes its default look hides. The tank's turret is hidden
        // with the flash on the bone below it, which its muzzle flash hides as well; the crate's
        // box and the marker's `NULL` draw nothing; the emitter, the absent model and the tree's,
        // which the fixture install lacks, are skipped, in the order of their objects' names.
        assert_eq!(
            imported.skipped_models,
            [
                SkippedModel {
                    name: "absent".to_owned(),
                    reason: SkipReason::Missing,
                },
                SkippedModel {
                    name: "spray".to_owned(),
                    reason: SkipReason::Emitter,
                },
                SkippedModel {
                    name: "ptoak01".to_owned(),
                    reason: SkipReason::Missing,
                },
            ]
        );
        // The flash's texture is in no archive: its triangle is untextured, and the import says so.
        assert_eq!(imported.missing_textures, ["gone.tga"]);
        assert_eq!(
            scratch.names("one/game/client/models"),
            ["rock01.glb", "tank.glb"]
        );
        let install = Install::open(&scratch.path("zh")).unwrap();
        let mut assets = ModelAssets::new(&install).unwrap();
        let Ok(Converted::Model(tank)) = ModelImport::convert("tank", &mut assets) else {
            panic!("the tank converts");
        };
        assert_eq!(scratch.names("one/game/client/materials"), ["tank_3.toml"]);
        assert_eq!(
            scratch.read_text("one/game/client/materials/tank_3.toml"),
            Toml::write(&tank.materials[0].file)
        );
        assert_eq!(
            scratch.read("one/game/client/models/tank.glb"),
            tank.glb.encode()
        );
        let scaled = |name: &str, scale: f32, hide: &[&str]| ClientModel {
            model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
            scale: ModelScale::new(scale).unwrap(),
            hide: hide.iter().map(|&node| node.to_owned()).collect(),
        };
        let model = |name: &str, hide: &[&str]| scaled(name, 1.0, hide);
        let unit = |models| ClientUnit { models };
        let looks = |looks: Vec<(&str, ClientUnit)>| {
            looks
                .into_iter()
                .map(|(name, unit)| (DeclaredName::new(name).unwrap(), unit))
                .collect()
        };
        let tank = || unit(vec![model("tank", &["tank.turret", "tank.flash"])]);
        // The rock and its reskin at the rock's scale, which the reskin cannot set; the tree's
        // look as a unit of both its models, and as the client's of its tree draw module's; the
        // fluff's as a unit of its hidden turret, with the flash below it, and as the client's of
        // none.
        let rock = || unit(vec![scaled("rock01", 1.5, &[])]);
        let expected = ClientUnits {
            units: looks(vec![
                ("bush", tank()),
                ("oak", unit(vec![model("tank", &[]), model("rock01", &[])])),
                ("rock", rock()),
                ("rockred", rock()),
                ("tank", tank()),
                ("tankcamo", tank()),
                ("tankred", unit(vec![model("tank", &[])])),
            ]),
            objects: looks(vec![
                ("bush", unit(vec![model("tank", &[])])),
                ("oak", unit(vec![model("rock01", &[])])),
                ("rock", rock()),
                ("rockred", rock()),
            ]),
        };
        assert_eq!(
            Toml::parse::<ClientUnits>(&scratch.read_text("one/game/client/units.toml")).unwrap(),
            expected
        );

        // Two objects whose names give one unit type, each with a model, are refused.
        let clash = "Object Rock!\nDraw = W3DPropDraw M\nModelName = Rock01\nEnd\nEnd\nObject Rock?\nDraw = W3DPropDraw M\nModelName = Rock01\nEnd\nEnd\n";
        scratch.write(
            "zh/c.BIG",
            big(&[
                ("c.ini", b"c"),
                ("Data\\INI\\Object\\Clash.ini", clash.as_bytes()),
            ]),
        );
        let known = version_of(&scratch);
        let clash = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        assert!(matches!(
            clash.write(&scratch.path("two")),
            Err(ZeroHourError::UnitTypeClash { first, second })
                if first == "Rock!" && second == "Rock?"
        ));
    }
}
