use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::{Path, PathBuf};

use campfire_store::{DirEntries, EntryKind, InputFile, InputRanges};
use sha2::{Digest, Sha256};

use crate::error::ImportError;
use crate::zero_hour::archive_path::ArchivePath;
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
    files: BTreeMap<ArchivePath, Located>,
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
    pub(crate) path: ArchivePath,
    pub(crate) folder: String,
}

/// A texture the install holds: its key, and its kind by its extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallTexture {
    pub(crate) path: ArchivePath,
    pub(crate) kind: TextureKind,
}

/// A texture's file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureKind {
    Dds,
    Tga,
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
        found.retain(|archive| ArchivePath::of(&archive.name).as_str() != SKIPPED);
        // The game compares paths ignoring case, `\` between names; two that differ only in case
        // keep their bytes' order, so one tree gives one order.
        found.sort_by(|a, b| {
            ArchivePath::of(&a.name)
                .cmp(&ArchivePath::of(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut archives = Vec::new();
        let mut files = BTreeMap::new();
        for Found { name, path } in found {
            let mut file = InputFile::ranges(&path).map_err(ImportError::Read)?;
            let archive = BigArchive::read(&mut file, &path)?;
            for entry in archive.entries {
                if let Entry::Vacant(vacant) = files.entry(ArchivePath::of(&entry.path)) {
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
    pub(crate) fn read(&mut self, path: &ArchivePath) -> Result<Vec<u8>, ImportError> {
        let located = *self
            .files
            .get(path)
            .ok_or_else(|| ImportError::ZeroHour(ZeroHourError::NotInArchives(path.to_string())))?;
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
            .filter_map(|path| {
                let names: Vec<&str> = path.names().collect();
                let ["maps", folder, file] = names[..] else {
                    return None;
                };
                (file.strip_suffix(".map") == Some(folder)).then(|| InstallMap {
                    path: path.clone(),
                    folder: folder.to_owned(),
                })
            })
            .collect()
    }

    /// Whether the archives hold a file at `path`.
    pub(crate) fn contains(&self, path: &ArchivePath) -> bool {
        self.files.contains_key(path)
    }

    /// The language folder the game reads first, `Data\<language>\`: the one whose `Art\` the
    /// archives hold; `None` if none does, and an error if several do, as the importer cannot
    /// tell which the player's game reads.
    pub(crate) fn language(&self) -> Result<Option<String>, ZeroHourError> {
        let mut languages: Vec<&str> = self
            .files
            .keys()
            .filter_map(|path| match path.names().collect::<Vec<_>>()[..] {
                ["data", language, "art", ..] => Some(language),
                _ => None,
            })
            .collect();
        languages.dedup();
        match languages[..] {
            [] => Ok(None),
            [language] => Ok(Some(language.to_owned())),
            _ => Err(ZeroHourError::Languages(
                languages
                    .iter()
                    .map(|language| (*language).to_owned())
                    .collect(),
            )),
        }
    }

    /// The object INI files, in the order `ThingFactory` loads them: for each of
    /// `Data\INI\Default\Object` and `Data\INI\Object`, the file of that name, then the files in
    /// that folder, then those in its folders below.
    pub(crate) fn object_inis(&self) -> Vec<ArchivePath> {
        let mut inis = Vec::new();
        for base in ["data\\ini\\default\\object", "data\\ini\\object"] {
            let file = ArchivePath::of(&format!("{base}.ini"));
            if self.contains(&file) {
                inis.push(file);
            }
            let depth = base.matches('\\').count() + 1;
            let folder = format!("{base}\\");
            let inside: Vec<&ArchivePath> = self
                .files
                .keys()
                .filter(|path| {
                    path.extension() == Some("ini") && path.as_str().starts_with(&folder)
                })
                .collect();
            let (direct, deeper): (Vec<_>, Vec<_>) = inside
                .into_iter()
                .partition(|path| path.names().count() == depth + 1);
            inis.extend(direct.into_iter().cloned());
            inis.extend(deeper.into_iter().cloned());
        }
        inis
    }

    /// The textures the install holds, each by its key once: every `.dds` and `.tga`, in the order
    /// of their keys.
    pub(crate) fn textures(&self) -> Vec<InstallTexture> {
        self.files
            .keys()
            .filter_map(|path| {
                let kind = match path.extension()? {
                    "dds" => TextureKind::Dds,
                    "tga" => TextureKind::Tga,
                    _ => return None,
                };
                Some(InstallTexture {
                    path: path.clone(),
                    kind,
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
            let big = Path::new(&entry.name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("big"));
            match entry.kind {
                EntryKind::Dir => Install::find(&path, &format!("{name}/"), found)?,
                EntryKind::File | EntryKind::Link if big => found.push(Found { name, path }),
                EntryKind::File | EntryKind::Link | EntryKind::Other => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_store::Scratch;

    use crate::texture::dds_file::internals::dds;
    use crate::texture::tga_file::internals::tga;
    use crate::zero_hour::big_archive::internals::big;
    use crate::zero_hour::map_file::internals::map;
    use crate::zero_hour::model_import::internals::{nothing_drawn, rock, tank};
    use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
    use crate::zero_hour::ref_pack::internals::literal;

    /// The fixture map's file, packed by `RefPack` as the game ships most maps.
    pub(crate) fn packed_map() -> Vec<u8> {
        let plain = map(8);
        let len = u32::try_from(plain.len()).unwrap().to_le_bytes();
        [&b"EAR\0"[..], &len, &literal(&plain)].concat()
    }

    /// A DXT1 texture of 8 × 8 texels and its full chain, and a TGA of 2 × 1.
    pub(crate) fn textures() -> [Vec<u8>; 2] {
        [
            dds(*b"DXT1", 8, 8, 4, &[7; 32 + 8 + 8 + 8]),
            tga(2, 1, 32, 0, &[1, 2, 3, 4, 5, 6, 7, 8]),
        ]
    }

    /// An install whose archives each hold `shared.ini`: `A.big`, which also holds the fixture
    /// map in its folder, a map outside one, the two textures, the object INI files, and the
    /// models, `Rock01.w3d` in the language folder `English` and, unread, outside it; `b.big`,
    /// `c.BIG`, base Generals' `ZH_Generals/base.big`, and the duplicate `Data/INI/INIZH.big`,
    /// beside a file that is no archive.
    pub(crate) fn fixture(scratch: &Scratch) {
        let packed = packed_map();
        let [rock_texture, sign] = textures();
        let [prop, spray] = nothing_drawn();
        scratch.write(
            "zh/A.big",
            big(&[
                ("Data\\a.ini", b"A1"),
                ("shared.ini", b"from A"),
                ("Maps\\Fixture Map\\Fixture Map.map", &packed),
                ("Maps\\Stray\\Other.map", b"no map the game lists"),
                ("Art\\Textures\\Rock.dds", &rock_texture),
                ("Art\\Textures\\Sign.tga", &sign),
                ("Data\\INI\\Object.ini", OBJECTS.as_bytes()),
                ("Data\\INI\\Object\\Deep\\More.ini", b""),
                ("Data\\INI\\Object\\Misc.ini", MORE_OBJECTS.as_bytes()),
                ("Data\\INI\\Object\\Notes.txt", b"no INI"),
                ("Data\\INI\\Default\\Object.ini", DEFAULT_OBJECTS.as_bytes()),
                ("Art\\W3D\\Tank.w3d", &tank()),
                ("Data\\English\\Art\\W3D\\Rock01.w3d", &rock()),
                ("Art\\W3D\\Rock01.w3d", b"unread"),
                ("Art\\W3D\\Prop.w3d", &prop),
                ("Art\\W3D\\Spray.w3d", &spray),
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
        assert_eq!(
            install.read(&ArchivePath::of("shared.ini")).unwrap(),
            b"from A"
        );
        assert_eq!(
            install.read(&ArchivePath::of("SHARED.INI")).unwrap(),
            b"from A"
        );
        assert_eq!(install.read(&ArchivePath::of("data/A.ini")).unwrap(), b"A1");
        assert_eq!(install.read(&ArchivePath::of("b\\B.INI")).unwrap(), b"b");
        assert_eq!(install.read(&ArchivePath::of("c.ini")).unwrap(), b"c");
        assert_eq!(
            install.read(&ArchivePath::of("base.ini")).unwrap(),
            b"base only"
        );
        let texture = |path: &str, kind| InstallTexture {
            path: ArchivePath::of(path),
            kind,
        };
        assert_eq!(
            install.textures(),
            [
                texture("art\\textures\\rock.dds", TextureKind::Dds),
                texture("art\\textures\\sign.tga", TextureKind::Tga),
            ]
        );
        // The language folder whose `Art` the archives hold; the object INI files in the game's
        // order: the default file, then the file, its folder's files, and its folders' below.
        assert_eq!(install.language().unwrap().as_deref(), Some("english"));
        assert_eq!(
            install.object_inis(),
            [
                "data\\ini\\default\\object.ini",
                "data\\ini\\object.ini",
                "data\\ini\\object\\misc.ini",
                "data\\ini\\object\\deep\\more.ini",
            ]
            .map(ArchivePath::of)
        );
        // The map in its own folder, by its key; not the one in another's.
        assert_eq!(
            install.maps(),
            [InstallMap {
                path: ArchivePath::of("maps\\fixture map\\fixture map.map"),
                folder: "fixture map".to_owned(),
            }]
        );
        assert!(matches!(
            install.read(&ArchivePath::of("missing.ini")),
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
        // A second language folder with `Art` leaves the importer unable to tell which one the
        // game reads.
        // Windows replaces no file a handle holds open, so the install closes before its archive is.
        drop(install);
        scratch.write("zh/c.BIG", big(&[("Data\\German\\Art\\W3D\\x.w3d", b"x")]));
        assert!(matches!(
            Install::open(&scratch.path("zh")).unwrap().language(),
            Err(ZeroHourError::Languages(languages)) if languages == ["english", "german"]
        ));
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
