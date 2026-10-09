use std::path::Path;

use campfire_common::Fingerprint;
use campfire_package::PackageWriter;

use crate::error::ImportError;
use crate::zero_hour::game_version::GameVersion;
use crate::zero_hour::install::Install;

pub(crate) mod big_archive;
pub(crate) mod error;
pub(crate) mod game_version;
pub(crate) mod install;
pub(crate) mod ref_pack;

/// A Zero Hour install, read as the game reads it and of a version the importer knows, which it
/// imports into a package.
#[derive(Debug)]
pub struct ZeroHour<'a> {
    #[expect(dead_code, reason = "the import of maps reads it")]
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

    /// Writes the install's package into the new directory `out`; its fingerprint.
    pub fn write(&mut self, out: &Path) -> Result<Fingerprint, ImportError> {
        let writer = PackageWriter::create(out).map_err(ImportError::Write)?;
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
    use crate::zero_hour::error::{VersionDifference, ZeroHourError};
    use crate::zero_hour::game_version::ArchiveHash;
    use crate::zero_hour::install::internals::fixture;

    #[test]
    fn an_install_of_a_known_version_imports_and_another_does_not() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let hash = |file: &str| -> [u8; 32] { Sha256::digest(scratch.read(file)).into() };
        let archives: &'static [ArchiveHash] = Box::leak(Box::new([
            ArchiveHash {
                path: "A.big",
                sha256: hash("zh/A.big"),
            },
            ArchiveHash {
                path: "b.big",
                sha256: hash("zh/b.big"),
            },
            ArchiveHash {
                path: "c.BIG",
                sha256: hash("zh/c.BIG"),
            },
            ArchiveHash {
                path: "ZH_Generals/base.big",
                sha256: hash("zh/ZH_Generals/base.big"),
            },
        ]));
        let known = [GameVersion {
            name: "fixture",
            archives,
        }];
        let mut game = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
        assert_eq!(game.version().name, "fixture");
        // A package of no file yet: its index is its tag and a count of 0. Two imports write the
        // same bytes.
        let empty = [&b"campfire/package-index/v1"[..], &[0]].concat();
        let fingerprint = game.write(&scratch.path("one")).unwrap();
        assert_eq!(fingerprint, Fingerprint::new(Sha256::digest(&empty).into()));
        assert_eq!(game.write(&scratch.path("two")).unwrap(), fingerprint);
        assert_eq!(scratch.read("two/package.index"), empty);
        assert_eq!(
            PackageDir::new(scratch.path("one"))
                .read()
                .unwrap()
                .fingerprint(),
            fingerprint
        );

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
