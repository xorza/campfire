use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::{Path, PathBuf};

use campfire_store::{DirEntries, EntryKind, InputFile, InputRanges};
use sha2::{Digest, Sha256};

use crate::error::ImportError;
use crate::zero_hour::big_archive::BigArchive;
use crate::zero_hour::error::ZeroHourError;

/// A Zero Hour install as the game reads it: every `.big` archive under its directory, its
/// subdirectories' included, in the case-insensitive order of their paths, and each file's path in
/// the first archive that holds it, as `loadBigFilesFromDirectory` loads them with `overwrite` off
/// (`StdBIGFileSystem` in the released source). Base Generals' archives, in Steam's `ZH_Generals/`,
/// sort after Zero Hour's own, so they fill only what Zero Hour lacks; the copy of `INIZH.big` some
/// versions left in `Data/INI/` is skipped, as the game skips it.
#[derive(Debug)]
pub(crate) struct Install {
    /// In the order the game loads them.
    archives: Vec<Archive>,
    /// Each file by its key, the first archive's.
    files: BTreeMap<String, Located>,
}

/// One archive: its path in the install, with `/` between names, and its file, open to read its
/// entries' ranges and its hash.
#[derive(Debug)]
struct Archive {
    name: String,
    file: InputRanges,
}

/// Where a file's bytes are: its archive, by its place in the load order, and their range.
#[derive(Debug, Clone, Copy)]
struct Located {
    archive: usize,
    offset: u64,
    size: u64,
}

/// A map the game lists: its path, as `maps\<folder>\<folder>.map`, and its folder's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallMap {
    pub(crate) path: String,
    pub(crate) folder: String,
}

/// One archive of the install and the SHA-256 of its bytes, as a version check compares them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HashedArchive {
    pub(crate) name: String,
    pub(crate) sha256: [u8; 32],
}

/// An archive the install holds: its path in the install, with `/` between names, and its path on
/// disk.
#[derive(Debug)]
struct Found {
    name: String,
    path: PathBuf,
}

/// The archive the game skips, by its key.
const SKIPPED: &str = "data\\ini\\inizh.big";

/// The bytes of an archive read at once as it is hashed.
const HASH_CHUNK: u64 = 1 << 20;

impl Install {
    /// The install under `root`, its archives read in the game's order.
    pub(crate) fn open(root: &Path) -> Result<Install, ImportError> {
        let mut found = Vec::new();
        Install::find(root, "", &mut found)?;
        found.retain(|archive| Install::key(&archive.name) != SKIPPED);
        // The game compares paths ignoring case, `\` between names; two that differ only in case
        // keep their bytes' order, so one tree gives one order.
        found.sort_by(|a, b| {
            Install::key(&a.name)
                .cmp(&Install::key(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut archives = Vec::new();
        let mut files = BTreeMap::new();
        for Found { name, path } in found {
            let mut file = InputFile::ranges(&path).map_err(ImportError::Read)?;
            let archive = BigArchive::read(&mut file, &path)?;
            for entry in archive.entries {
                if let Entry::Vacant(vacant) = files.entry(Install::key(&entry.path)) {
                    vacant.insert(Located {
                        archive: archives.len(),
                        offset: entry.offset,
                        size: entry.size,
                    });
                }
            }
            archives.push(Archive { name, file });
        }
        Ok(Install { archives, files })
    }

    /// The bytes of the file at `path`, as the game names it, in the first archive that holds
    /// it.
    pub(crate) fn read(&mut self, path: &str) -> Result<Vec<u8>, ImportError> {
        let located = *self
            .files
            .get(&Install::key(path))
            .ok_or_else(|| ImportError::ZeroHour(ZeroHourError::NotInArchives(path.to_owned())))?;
        let len = usize::try_from(located.size).expect("a u32 fits usize");
        self.archives[located.archive]
            .file
            .read_at(located.offset, len)
            .map_err(ImportError::Read)
    }

    /// The maps the game lists, as `MapCache::loadMapsFromDisk` finds them: each `.map` under
    /// `Maps\` in a folder of its own name, in the order of their keys; one elsewhere is none.
    pub(crate) fn maps(&self) -> Vec<InstallMap> {
        self.files
            .keys()
            .filter_map(|key| {
                let folder = key.strip_prefix("maps\\")?.split_once('\\')?.0;
                (*key == format!("maps\\{folder}\\{folder}.map")).then(|| InstallMap {
                    path: key.clone(),
                    folder: folder.to_owned(),
                })
            })
            .collect()
    }

    /// Each archive's path and SHA-256, in the load order, each hashed through the handle that
    /// reads its entries, so the bytes a version check knows are the bytes the import reads.
    pub(crate) fn hashed(&mut self) -> Result<Vec<HashedArchive>, ImportError> {
        self.archives
            .iter_mut()
            .map(|archive| {
                let mut hasher = Sha256::new();
                let mut at = 0;
                while at < archive.file.len() {
                    let len = (archive.file.len() - at).min(HASH_CHUNK);
                    let chunk = usize::try_from(len).expect("a chunk fits usize");
                    let bytes = archive.file.read_at(at, chunk).map_err(ImportError::Read)?;
                    hasher.update(bytes);
                    at += len;
                }
                Ok(HashedArchive {
                    name: archive.name.clone(),
                    sha256: hasher.finalize().into(),
                })
            })
            .collect()
    }

    /// The archives under `dir`, `at` its path in the install: each file, or link to one, whose
    /// name ends in `.big`, ignoring case, as the game's search finds them. A name that is not
    /// UTF-8 sorts by its lossy text, and opens by its own.
    fn find(dir: &Path, at: &str, found: &mut Vec<Found>) -> Result<(), ImportError> {
        for entry in DirEntries::read(dir).map_err(ImportError::Read)? {
            let name = format!("{at}{}", entry.name.to_string_lossy());
            let path = dir.join(&entry.name);
            let big = name.to_ascii_lowercase().ends_with(".big");
            match entry.kind {
                EntryKind::Dir => Install::find(&path, &format!("{name}/"), found)?,
                EntryKind::File | EntryKind::Link if big => found.push(Found { name, path }),
                EntryKind::File | EntryKind::Link | EntryKind::Other => {}
            }
        }
        Ok(())
    }

    /// The key the game finds a path by: its ASCII lowercase, with `\` between names.
    fn key(path: &str) -> String {
        path.replace('/', "\\").to_ascii_lowercase()
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_store::Scratch;

    use crate::zero_hour::big_archive::internals::big;
    use crate::zero_hour::map_file::internals::map;
    use crate::zero_hour::ref_pack::internals::literal;

    /// The fixture map's file, packed by `RefPack` as the game ships most maps.
    pub(crate) fn packed_map() -> Vec<u8> {
        let plain = map(8);
        let len = u32::try_from(plain.len()).unwrap().to_le_bytes();
        [&b"EAR\0"[..], &len, &literal(&plain)].concat()
    }

    /// An install whose archives each hold `shared.ini`: `A.big`, which also holds the fixture
    /// map in its folder and a map outside one, `b.big`, `c.BIG`, base Generals'
    /// `ZH_Generals/base.big`, and the duplicate `Data/INI/INIZH.big`, beside a file that is no
    /// archive.
    pub(crate) fn fixture(scratch: &Scratch) {
        let packed = packed_map();
        scratch.write(
            "zh/A.big",
            big(&[
                ("Data\\a.ini", b"A1"),
                ("shared.ini", b"from A"),
                ("Maps\\Fixture Map\\Fixture Map.map", &packed),
                ("Maps\\Stray\\Other.map", b"no map the game lists"),
            ]),
        );
        scratch.write(
            "zh/b.big",
            big(&[("shared.ini", b"from b"), ("B\\b.ini", b"b")]),
        );
        scratch.write("zh/c.BIG", big(&[("c.ini", b"c")]));
        scratch.write("zh/Data/INI/INIZH.big", big(&[("shared.ini", b"dup")]));
        scratch.write(
            "zh/ZH_Generals/base.big",
            big(&[("shared.ini", b"base"), ("base.ini", b"base only")]),
        );
        scratch.write("zh/readme.txt", "no archive");
    }
}

#[cfg(test)]
mod tests {
    use campfire_store::Scratch;

    use super::*;
    use crate::zero_hour::big_archive::internals::big;
    use crate::zero_hour::error::ArchiveError;
    use crate::zero_hour::install::internals::fixture;

    #[test]
    fn an_install_reads_each_file_from_the_first_archive_that_holds_it() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let mut install = Install::open(&scratch.path("zh")).unwrap();
        // The order, ignoring case: `a.big`, `b.big`, `c.big`, `zh_generals\base.big`; the
        // duplicate skipped, the text file no archive.
        let names: Vec<String> = install
            .archives
            .iter()
            .map(|archive| archive.name.clone())
            .collect();
        assert_eq!(names, ["A.big", "b.big", "c.BIG", "ZH_Generals/base.big"]);
        assert_eq!(install.read("shared.ini").unwrap(), b"from A");
        assert_eq!(install.read("SHARED.INI").unwrap(), b"from A");
        assert_eq!(install.read("data/A.ini").unwrap(), b"A1");
        assert_eq!(install.read("b\\B.INI").unwrap(), b"b");
        assert_eq!(install.read("c.ini").unwrap(), b"c");
        assert_eq!(install.read("base.ini").unwrap(), b"base only");
        // The map in its own folder, by its key; not the one in another's.
        assert_eq!(
            install.maps(),
            [InstallMap {
                path: "maps\\fixture map\\fixture map.map".to_owned(),
                folder: "fixture map".to_owned(),
            }]
        );
        assert!(matches!(
            install.read("missing.ini"),
            Err(ImportError::ZeroHour(ZeroHourError::NotInArchives(path))) if path == "missing.ini"
        ));
        // Each archive's hash, in the load order.
        let hashed = install.hashed().unwrap();
        let expected: Vec<HashedArchive> = [
            "zh/A.big",
            "zh/b.big",
            "zh/c.BIG",
            "zh/ZH_Generals/base.big",
        ]
        .into_iter()
        .zip(&names)
        .map(|(file, name)| HashedArchive {
            name: name.clone(),
            sha256: Sha256::digest(scratch.read(file)).into(),
        })
        .collect();
        assert_eq!(hashed, expected);
    }

    #[test]
    fn an_archive_the_game_cannot_read_fails_the_install() {
        let full = big(&[("a.ini", b"abc")]);
        // Its tag, a header past the file, and an entry past the file.
        let mut wrong = full.clone();
        wrong[3] = b'G';
        let mut long_header = full.clone();
        long_header[15] = 200;
        let cut = full[..full.len() - 1].to_vec();
        for (bytes, expected) in [
            (wrong, ArchiveError::NotBig),
            (long_header, ArchiveError::Short),
            (cut, ArchiveError::EntryPastEnd("a.ini".to_owned())),
            (b"BIG".to_vec(), ArchiveError::Short),
        ] {
            let scratch = Scratch::new();
            scratch.write("zh/x.big", bytes);
            let error = Install::open(&scratch.path("zh")).unwrap_err();
            assert!(
                matches!(
                    &error,
                    ImportError::ZeroHour(ZeroHourError::Archive { archive, error })
                        if *archive == scratch.path("zh/x.big") && *error == expected
                ),
                "{error:?}"
            );
        }
    }
}
