use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use campfire_capabilities::PackagePath;
use campfire_common::Toml;
use campfire_common::{Fingerprint, MapName};
use campfire_package::{
    CameraFile, ClientModel, ClientUnit, ClientUnits, GameData, PackageWriter, TextureClass,
    WriteError,
};
use campfire_store::DurableFile;

use crate::error::ImportError;
use crate::texture::dds_file::DdsFile;
use crate::texture::tga_file::TgaFile;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::{IniError, ZeroHourError};
use crate::zero_hour::game_data_ini::GameDataIni;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::install::{Install, InstallTexture, TextureKind};
use crate::zero_hour::map_file::MapFile;
use crate::zero_hour::map_import::MapImport;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_parts::ModelParts;
use crate::zero_hour::object_ini::ObjectIni;
use crate::zero_hour::rules_package::{MODE_DATA, MODE_MANIFEST, RULES_FILES};
use crate::zero_hour::terrain_ini::TerrainIni;
use crate::zero_hour::unit_types::UnitTypes;

pub(crate) mod archive_path;
pub(crate) mod big_archive;
pub(crate) mod chunk_reader;
pub(crate) mod error;
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
pub(crate) mod object_ini;
pub(crate) mod pose;
pub(crate) mod ref_pack;
pub(crate) mod rules_package;
pub(crate) mod terrain_ini;
pub(crate) mod unit_types;
pub(crate) mod w3d_file;
pub(crate) mod w3d_mesh;

/// What an import wrote: the package's fingerprint, the models the objects name that the
/// package lacks, and the textures its models name that no archive holds, which they draw
/// untextured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The imported package's.
    pub fingerprint: Fingerprint,
    /// The rules package's, beside it.
    pub rules: Fingerprint,
    pub skipped_models: Vec<SkippedModel>,
    pub missing_textures: Vec<String>,
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
    pub fn open(root: &Path, known: &'a [GameVersion]) -> Result<ZeroHour<'a>, ImportError> {
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
    pub fn write(&self, out: &Path) -> Result<Imported, ImportError> {
        DurableFile::create_new_dir(out)
            .map_err(WriteError::CreateDir)
            .map_err(ImportError::Write)?;
        let rules = ZeroHour::write_rules(&out.join(ZeroHour::RULES))?;
        let mut writer =
            PackageWriter::create(&out.join(ZeroHour::GAME)).map_err(ImportError::Write)?;
        let mut write = |path: &str, bytes: &[u8]| {
            let path = PackagePath::parse(path).expect("the import's paths are package paths");
            writer.write(path, bytes).map_err(ImportError::Write)
        };
        write("manifest.toml", MODE_MANIFEST.as_bytes())?;
        write("data/mode.toml", MODE_DATA.as_bytes())?;
        let terrain_types = self.read_inis("Terrain", TerrainIni::default(), TerrainIni::read)?;
        let game_data = self.read_inis("GameData", GameDataIni::default(), GameDataIni::read)?;
        let text = Toml::write(&game_data.game_data()).expect("game data writes as TOML");
        write(GameData::PATH, text.as_bytes())?;
        let camera = game_data
            .camera_file()
            .ok_or(ImportError::ZeroHour(ZeroHourError::Camera))?;
        let text = Toml::write(&camera).expect("a camera writes as TOML");
        write(CameraFile::PATH, text.as_bytes())?;
        let mut unit_types = UnitTypes::default();
        let mut names = BTreeMap::<MapName, String>::new();
        for map in self.install.maps() {
            let name = ImportName::of(map.folder.as_bytes()).map();
            if let Some(first) = names.insert(name.clone(), map.path.to_string()) {
                return Err(ImportError::ZeroHour(ZeroHourError::MapNameClash {
                    first,
                    second: map.path.to_string(),
                }));
            }
            let bytes = self.install.read(&map.path)?;
            let imported = MapFile::read(&bytes)
                .and_then(|mut file| {
                    let terrain = &mut file.terrain;
                    for class in terrain.classes.iter_mut().chain(&mut terrain.edge_classes) {
                        class.texture = self.class_texture(class, &terrain_types);
                    }
                    MapImport::new(&file, &mut unit_types)
                })
                .map_err(|error| {
                    ImportError::ZeroHour(ZeroHourError::Map {
                        map: map.path.to_string(),
                        error,
                    })
                })?;
            write(&format!("map/{name}/map.toml"), imported.map.as_bytes())?;
            write(&format!("map/{name}/heights.bin"), &imported.heights)?;
            write(
                &format!("client/maps/{name}/terrain.bin"),
                &imported.terrain,
            )?;
        }
        for InstallTexture { path, kind } in self.install.textures() {
            let bytes = self.install.read(&path)?;
            let texture = |error| {
                ImportError::ZeroHour(ZeroHourError::Texture {
                    path: path.to_string(),
                    error,
                })
            };
            let ktx2 = match kind {
                TextureKind::Dds => DdsFile::read(&bytes).map_err(texture)?.ktx2(),
                TextureKind::Tga => TgaFile::read(&bytes).map_err(texture)?.ktx2(),
            };
            write(ZeroHour::texture_path(&path).as_str(), &ktx2)?;
        }
        write("data/units.toml", unit_types.toml().as_bytes())?;
        let mut skipped_models = Vec::new();
        let mut missing_textures = BTreeSet::new();
        self.write_models(&mut write, &mut skipped_models, &mut missing_textures)?;
        Ok(Imported {
            fingerprint: writer.finish().map_err(ImportError::Write)?,
            rules,
            skipped_models,
            missing_textures: missing_textures.into_iter().collect(),
        })
    }

    /// Writes each object's models: each model once, at `client/models/<name>.glb`, with the
    /// material files its shaders need, and `client/units.toml`, each object's models and the
    /// nodes its default look hides; adds the models the objects name that the package lacks to
    /// `skipped`, and the textures the models name that no archive holds to `missing_textures`.
    fn write_models(
        &self,
        write: &mut impl FnMut(&str, &[u8]) -> Result<(), ImportError>,
        skipped: &mut Vec<SkippedModel>,
        missing_textures: &mut BTreeSet<String>,
    ) -> Result<(), ImportError> {
        let ini = self.read_inis("Object", ObjectIni::default(), ObjectIni::read)?;
        let mut assets = ModelAssets::new(&self.install).map_err(ImportError::ZeroHour)?;
        let mut models: BTreeMap<String, Option<ModelParts>> = BTreeMap::new();
        let mut units = ClientUnits::default();
        let mut types = BTreeMap::<String, String>::new();
        for object in ini.objects() {
            let mut entries = Vec::new();
            for draw in ini.draws(object) {
                let Some(model) = &draw.model else {
                    continue;
                };
                let name = model.to_ascii_lowercase();
                let parts = match models.entry(name.clone()) {
                    Entry::Occupied(known) => known.into_mut(),
                    Entry::Vacant(new) => new.insert(ZeroHour::write_model(
                        &name,
                        &mut assets,
                        write,
                        skipped,
                        missing_textures,
                    )?),
                };
                if let Some(parts) = parts {
                    entries.push(ClientModel {
                        model: ZeroHour::model_path(&name),
                        hide: parts.hidden(&draw.look),
                    });
                }
            }
            if entries.is_empty() {
                continue;
            }
            let unit_type = ImportName::of(object.name.as_bytes());
            if let Some(first) = types.insert(unit_type.as_str().to_owned(), object.name.clone()) {
                return Err(ImportError::ZeroHour(ZeroHourError::UnitTypeClash {
                    first,
                    second: object.name.clone(),
                }));
            }
            units
                .units
                .insert(unit_type.declared(), ClientUnit { models: entries });
        }
        let text = Toml::write(&units).expect("client units write as TOML");
        write("client/units.toml", text.as_bytes())
    }

    /// Converts the model `name` and writes its files; the parts its default looks read, or
    /// `None` if it draws nothing or is skipped, which `skipped` gains.
    fn write_model(
        name: &str,
        assets: &mut ModelAssets<'_>,
        write: &mut impl FnMut(&str, &[u8]) -> Result<(), ImportError>,
        skipped: &mut Vec<SkippedModel>,
        missing_textures: &mut BTreeSet<String>,
    ) -> Result<Option<ModelParts>, ImportError> {
        let reason = match ModelImport::convert(name, assets)? {
            Converted::Model(import) => {
                write(ZeroHour::model_path(name).as_str(), &import.glb.encode())?;
                for material in &import.materials {
                    let text = Toml::write(&material.file).expect("a material file writes as TOML");
                    write(
                        &format!("client/materials/{}.toml", material.name),
                        text.as_bytes(),
                    )?;
                }
                missing_textures.extend(import.missing_textures);
                return Ok(Some(import.parts));
            }
            Converted::Nothing => return Ok(None),
            Converted::Emitter => SkipReason::Emitter,
            Converted::Missing => SkipReason::Missing,
        };
        skipped.push(SkippedModel {
            name: name.to_owned(),
            reason,
        });
        Ok(None)
    }
}

impl ZeroHour<'_> {
    /// The directory of the rules package, and of the install's, in an import's.
    pub const RULES: &'static str = "zero-hour";
    pub const GAME: &'static str = "game";

    /// Writes the rules package into the new directory `dir`; its fingerprint.
    fn write_rules(dir: &Path) -> Result<Fingerprint, ImportError> {
        let mut writer = PackageWriter::create(dir).map_err(ImportError::Write)?;
        for (path, bytes) in RULES_FILES {
            let path =
                PackagePath::parse(path).expect("the rules package's paths are package paths");
            writer.write(path, bytes).map_err(ImportError::Write)?;
        }
        writer.finish().map_err(ImportError::Write)
    }

    /// `into` after `read` read each INI file of `name`, in the game's order.
    fn read_inis<T>(
        &self,
        name: &str,
        mut into: T,
        read: fn(&mut T, &[u8]) -> Result<(), IniError>,
    ) -> Result<T, ImportError> {
        for path in self.install.inis(name) {
            let bytes = self.install.read(&path)?;
            read(&mut into, &bytes).map_err(|error| {
                ImportError::ZeroHour(ZeroHourError::Ini {
                    path: path.to_string(),
                    error,
                })
            })?;
        }
        Ok(into)
    }

    /// The package's texture of `class`: the file `Art\Terrain\` holds by the name its terrain
    /// type gives, as `readTexClass` opens it; none for a class no type names, or a file the
    /// install lacks.
    fn class_texture(&self, class: &TextureClass, types: &TerrainIni) -> Option<PackagePath> {
        let file = types.texture(&class.name)?;
        let path = ArchivePath::of(&format!("art\\terrain\\{file}"));
        self.install
            .contains(&path)
            .then(|| ZeroHour::texture_path(&path))
    }

    /// Where the texture at `path` in the archives goes: `client/textures/` and its path with `/`
    /// between names, its extension `.ktx2`.
    pub(crate) fn texture_path(path: &ArchivePath) -> PackagePath {
        let path = format!(
            "client/textures/{}.ktx2",
            path.without_extension().replace('\\', "/")
        );
        PackagePath::parse(&path).expect("an archive path is a package path")
    }

    /// Where the model `name` goes: `client/models/<name>.glb`.
    fn model_path(name: &str) -> PackagePath {
        PackagePath::parse(&format!("client/models/{name}.glb"))
            .expect("a model name is a package path")
    }
}

#[cfg(test)]
mod tests {
    use campfire_package::{ModePackages, PackageDir};
    use campfire_store::Scratch;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::texture::internals::ktx2;
    use crate::zero_hour::big_archive::internals::big;
    use crate::zero_hour::error::VersionDifference;
    use crate::zero_hour::game_version::ArchiveHash;
    use crate::zero_hour::install::internals::{fixture, packed_map};
    use crate::zero_hour::map_file::internals::map;
    use campfire_capabilities::DeclaredName;

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
        let game = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        assert_eq!(game.version().name, "fixture");

        // The fixture map, in its folder `Fixture Map`, is `fixture_map`: its files are the ones
        // its import gives, and its two unit types the package's.
        let imported = game.write(&scratch.path("one")).unwrap();
        let fingerprint = imported.fingerprint;
        // Beside it, the rules package as the workspace holds it, which the imported mode, of
        // its manifest and mode data, depends on and loads with.
        assert_eq!(scratch.names("one"), ["game", "zero-hour"]);
        assert_eq!(
            scratch.names("one/game"),
            ["client", "data", "manifest.toml", "map", "package.index"]
        );
        assert_eq!(
            scratch.read("one/zero-hour/package.index"),
            include_bytes!("../../../../packages/zero-hour/package.index")
        );
        let named = MapName::new("fixture_map").unwrap();
        let mode = ModePackages::from_dir(&scratch.path("one/game"), &named);
        assert!(mode.is_ok(), "{mode:?}");
        assert_eq!(scratch.names("one/game/map"), ["fixture_map"]);
        assert_eq!(
            scratch.names("one/game/map/fixture_map"),
            ["heights.bin", "map.toml"]
        );
        assert_eq!(
            scratch.names("one/game/client/maps/fixture_map"),
            ["terrain.bin"]
        );
        // The class whose type's texture the install holds names it; the other names none.
        let mut file = MapFile::read(&map(8)).unwrap();
        let sand = PackagePath::parse("client/textures/art/terrain/tsand.ktx2").unwrap();
        file.terrain.classes[0].texture = Some(sand);
        let mut types = UnitTypes::default();
        let expected = MapImport::new(&file, &mut types).unwrap();
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
        assert_eq!(scratch.read_text("one/game/data/units.toml"), types.toml());
        let camera = Toml::parse::<CameraFile>(&scratch.read_text("one/game/client/camera.toml"));
        assert_eq!(camera.unwrap().field_of_view, 50.0);
        assert_eq!(
            Toml::parse::<GameData>(&scratch.read_text("one/game/client/game_data.toml")).unwrap(),
            GameData {
                adjust_cliff_textures: true,
                three_way_blends: false,
            }
        );
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
            game.write(&scratch.path("two")).unwrap().fingerprint,
            fingerprint
        );
        assert_eq!(
            scratch.read("two/game/package.index"),
            scratch.read("one/game/package.index")
        );
        assert_eq!(
            PackageDir::new(scratch.path("one/game"))
                .read()
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
            Err(ImportError::ZeroHour(ZeroHourError::MapNameClash { first, second }))
                if first == "maps\\fixture map\\fixture map.map"
                    && second == "maps\\fixture_map\\fixture_map.map"
        ));

        // An archive of other bytes is no version the importer knows.
        scratch.write("zh/c.BIG", b"BIGF");
        assert!(matches!(
            ZeroHour::open(&scratch.path("zh"), &known),
            Err(ImportError::ZeroHour(ZeroHourError::Archive { .. }))
        ));
        scratch.write("zh/c.BIG", big(&[("c.ini", b"C")]));
        assert!(matches!(
            ZeroHour::open(&scratch.path("zh"), &known),
            Err(ImportError::ZeroHour(ZeroHourError::UnknownVersion {
                archive,
                difference: VersionDifference::OtherBytes,
            })) if archive == "c.big"
        ));
    }

    #[test]
    fn an_import_writes_each_model_once_and_each_object_s_models() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let known = version_of(&scratch);
        let game = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        let imported = game.write(&scratch.path("one")).unwrap();
        // Each model the objects name once, as its conversion gives it; its material file; and
        // each object's models with the nodes its default look hides. The tank's turret is hidden
        // with the flash on the bone below it, which its muzzle flash hides as well; the crate's
        // box and the marker's `NULL` draw nothing; the emitter and the absent model are skipped.
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
            Toml::write(&tank.materials[0].file).unwrap()
        );
        assert_eq!(
            scratch.read("one/game/client/models/tank.glb"),
            tank.glb.encode()
        );
        let model = |name: &str, hide: &[&str]| ClientModel {
            model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
            hide: hide.iter().map(|&node| node.to_owned()).collect(),
        };
        let unit = |models| ClientUnit { models };
        let tank = || unit(vec![model("tank", &["tank.turret", "tank.flash"])]);
        let expected = ClientUnits {
            units: [
                ("rock", unit(vec![model("rock01", &[])])),
                ("tank", tank()),
                ("tankcamo", tank()),
                ("tankred", unit(vec![model("tank", &[])])),
            ]
            .into_iter()
            .map(|(name, unit)| (DeclaredName::new(name).unwrap(), unit))
            .collect(),
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
            Err(ImportError::ZeroHour(ZeroHourError::UnitTypeClash { first, second }))
                if first == "Rock!" && second == "Rock?"
        ));
    }
}
