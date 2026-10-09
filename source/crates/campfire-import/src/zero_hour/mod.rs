use std::collections::BTreeMap;
use std::path::Path;

use campfire_capabilities::PackagePath;
use campfire_common::{Fingerprint, MapName};
use campfire_package::PackageWriter;

use crate::error::ImportError;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::install::Install;
use crate::zero_hour::map_file::MapFile;
use crate::zero_hour::map_import::MapImport;
use crate::zero_hour::unit_types::UnitTypes;

pub(crate) mod big_archive;
pub(crate) mod chunk_reader;
pub(crate) mod error;
pub(crate) mod game_version;
pub(crate) mod import_name;
pub(crate) mod install;
pub(crate) mod map_file;
pub(crate) mod map_import;
pub(crate) mod map_object;
pub(crate) mod ref_pack;
pub(crate) mod unit_types;

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
        let mut install = Install::open(root)?;
        let version = GameVersion::identify(&install.hashed()?, known)?;
        Ok(ZeroHour { install, version })
    }

    /// The version of the install.
    pub const fn version(&self) -> &'a GameVersion {
        self.version
    }

    /// Writes the install's package into the new directory `out`; its fingerprint. Each map the
    /// game lists becomes `map/<name>/` and `client/maps/<name>/`, its name its folder's, and
    /// every template its objects name a unit type of `data/units.toml`.
    pub fn write(&mut self, out: &Path) -> Result<Fingerprint, ImportError> {
        let mut writer = PackageWriter::create(out).map_err(ImportError::Write)?;
        let mut write = |path: &str, bytes: &[u8]| {
            let path = PackagePath::parse(path).expect("the import's paths are package paths");
            writer.write(path, bytes).map_err(ImportError::Write)
        };
        let mut unit_types = UnitTypes::default();
        let mut names = BTreeMap::<MapName, String>::new();
        for map in self.install.maps() {
            let name = ImportName::of(map.folder.as_bytes()).map();
            if let Some(first) = names.insert(name.clone(), map.path.clone()) {
                return Err(ImportError::ZeroHour(ZeroHourError::MapNameClash {
                    first,
                    second: map.path,
                }));
            }
            let bytes = self.install.read(&map.path)?;
            let imported = MapFile::read(&bytes)
                .and_then(|file| MapImport::new(&file, &mut unit_types))
                .map_err(|error| {
                    ImportError::ZeroHour(ZeroHourError::Map {
                        map: map.path.clone(),
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
        write("data/units.toml", unit_types.toml().as_bytes())?;
        writer.finish().map_err(ImportError::Write)
    }
}

#[cfg(test)]
mod tests {
    use campfire_package::PackageDir;
    use campfire_store::Scratch;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::zero_hour::big_archive::internals::big;
    use crate::zero_hour::error::VersionDifference;
    use crate::zero_hour::game_version::ArchiveHash;
    use crate::zero_hour::install::internals::{fixture, packed_map};
    use crate::zero_hour::map_file::internals::map;

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
        let mut game = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        assert_eq!(game.version().name, "fixture");

        // The fixture map, in its folder `Fixture Map`, is `fixture_map`: its files are the ones
        // its import gives, and its two unit types the package's.
        let fingerprint = game.write(&scratch.path("one")).unwrap();
        assert_eq!(
            scratch.names("one"),
            ["client", "data", "map", "package.index"]
        );
        assert_eq!(scratch.names("one/map"), ["fixture_map"]);
        assert_eq!(
            scratch.names("one/map/fixture_map"),
            ["heights.bin", "map.toml"]
        );
        assert_eq!(
            scratch.names("one/client/maps/fixture_map"),
            ["terrain.bin"]
        );
        let mut types = UnitTypes::default();
        let expected = MapImport::new(&MapFile::read(&map(8)).unwrap(), &mut types).unwrap();
        assert_eq!(
            scratch.read_text("one/map/fixture_map/map.toml"),
            expected.map
        );
        assert_eq!(
            scratch.read("one/map/fixture_map/heights.bin"),
            expected.heights
        );
        assert_eq!(
            scratch.read("one/client/maps/fixture_map/terrain.bin"),
            expected.terrain
        );
        assert_eq!(scratch.read_text("one/data/units.toml"), types.toml());
        // Two imports write the same bytes, and the package reads as the fingerprint names it.
        assert_eq!(game.write(&scratch.path("two")).unwrap(), fingerprint);
        assert_eq!(
            scratch.read("two/package.index"),
            scratch.read("one/package.index")
        );
        assert_eq!(
            PackageDir::new(scratch.path("one"))
                .read()
                .unwrap()
                .fingerprint(),
            fingerprint
        );

        // A second folder whose name gives `fixture_map` is refused.
        scratch.write(
            "zh/c.BIG",
            big(&[
                ("c.ini", b"c"),
                ("Maps\\Fixture_Map\\Fixture_Map.map", &packed_map()),
            ]),
        );
        let known = version_of(&scratch);
        let mut clash = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
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
}
