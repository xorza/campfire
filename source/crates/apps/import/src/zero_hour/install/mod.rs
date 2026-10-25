use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::{Path, PathBuf};

use campfire_package_format::{ContentHash, ContentHashMeaning, ContentHasher};
use campfire_store::{
    DirEntries, DirEntry, EntryKind, InputFile, InputRanges, PathError, ReadError,
};
use campfire_threads::{Job, Parallel};

use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::big::big_archive::BigArchive;
use crate::zero_hour::format::big::error::BigReadError;
use crate::zero_hour::format::ini::error::IniError;

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

/// One archive: its path in the install, with `/` between names, and its file, open once as its
/// entries are read, which every read and the version check's hash take; a read refuses it when
/// another program changed it since, so every read is of the bytes the version check hashed.
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

/// A map the game lists: its path, as `maps\<directory>\<directory>.map`, and its directory's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallMap {
    pub(crate) path: ArchivePath,
    pub(crate) dir: String,
}

/// A texture's file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureKind {
    Dds,
    Tga,
}

impl TextureKind {
    /// The format of the texture at `path`, by its extension; none for a file of another kind.
    pub(crate) fn of(path: &ArchivePath) -> Option<Self> {
        match path.extension()? {
            "dds" => Some(Self::Dds),
            "tga" => Some(Self::Tga),
            _ => None,
        }
    }
}

/// One archive of the install and the SHA-256 of its bytes, as a version check compares them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HashedArchive {
    pub(crate) name: String,
    pub(crate) sha256: ContentHash,
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
    /// The install under `root`, its archives read in the game's order: a root whose last name
    /// is a link, as a player's install may be, by its target, as a listing follows no link, and
    /// any other root by the path it is given.
    pub(crate) fn open(root: &Path) -> Result<Self, ZeroHourError> {
        let (root, entries) = match DirEntries::read(root) {
            Ok(entries) => (root.to_owned(), entries),
            Err(PathError {
                error: ReadError::Link,
                ..
            }) => {
                let target = DirEntries::link_target(root)?;
                let entries = DirEntries::read(&target)?;
                (target, entries)
            }
            Err(error) => return Err(error.into()),
        };
        let mut found = Self::find(root, entries)?;
        found.retain(|archive| ArchivePath::of(&archive.name).as_str() != SKIPPED);
        // The game compares paths ignoring case, `\` between names; two that differ only in case
        // keep their bytes' order, so one tree gives one order.
        found.sort_by(|a, b| {
            ArchivePath::of(&a.name)
                .cmp(&ArchivePath::of(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });
        if let Some([first, second]) = found
            .array_windows()
            .find(|[first, second]| ArchivePath::of(&first.name) == ArchivePath::of(&second.name))
        {
            return Err(ZeroHourError::ArchiveClash {
                first: first.name.clone(),
                second: second.name.clone(),
            });
        }
        let mut archives = Vec::new();
        let mut files = BTreeMap::new();
        for Found { name, path } in found {
            let file = InputFile::ranges(&path)?;
            let big = BigArchive::read(&file).map_err(|error| match error {
                BigReadError::Read(error) => ZeroHourError::Read(error),
                BigReadError::Archive(error) => ZeroHourError::Archive {
                    archive: path.clone(),
                    error,
                },
            })?;
            for entry in big.entries {
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
        Ok(Self { archives, files })
    }

    /// `into` after `read` read each INI file of `name`, in the game's order.
    pub(crate) fn read_inis<T>(
        &self,
        name: &str,
        mut into: T,
        read: fn(&mut T, &[u8]) -> Result<(), IniError>,
    ) -> Result<T, ZeroHourError> {
        for path in self.inis(name) {
            let bytes = self.read(&path)?;
            read(&mut into, &bytes).map_err(|error| ZeroHourError::Ini {
                path: path.clone(),
                error,
            })?;
        }
        Ok(into)
    }

    /// The bytes of the file at `path`, as the game names it, in the first archive that holds
    /// it.
    pub(crate) fn read(&self, path: &ArchivePath) -> Result<Vec<u8>, ZeroHourError> {
        let located = *self
            .files
            .get(path)
            .ok_or_else(|| ZeroHourError::NotInArchives(path.clone()))?;
        let len = usize::try_from(located.size).expect("a u32 fits usize");
        self.archives[located.archive].read_at(located.offset, len)
    }

    /// The maps the game lists, as `MapCache::loadMapsFromDisk` finds them: each `.map` under
    /// `Maps\` in a directory of its own name, in the order of their keys; one elsewhere is none.
    pub(crate) fn maps(&self) -> Vec<InstallMap> {
        self.under("maps\\")
            .filter_map(|path| {
                let names: Vec<&str> = path.names().collect();
                let ["maps", map_dir, file] = names[..] else {
                    return None;
                };
                (file.strip_suffix(".map") == Some(map_dir)).then(|| InstallMap {
                    path: path.clone(),
                    dir: map_dir.to_owned(),
                })
            })
            .collect()
    }

    /// The keys of the files under `prefix`, a key's start that ends in `\\`, in their order: a
    /// range of the keys, as their order is their text's.
    fn under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a ArchivePath> {
        self.files
            .range(ArchivePath::of(prefix)..)
            .map(|(path, _)| path)
            .take_while(move |path| path.as_str().starts_with(prefix))
    }

    /// Whether the archives hold a file at `path`.
    pub(crate) fn contains(&self, path: &ArchivePath) -> bool {
        self.files.contains_key(path)
    }

    /// The language directory the game reads first, `Data\<language>\`: the one whose `Art\` the
    /// archives hold; `None` if none does, and an error if several do, as the importer cannot
    /// tell which the player's game reads.
    pub(crate) fn language(&self) -> Result<Option<String>, ZeroHourError> {
        let mut languages: Vec<&str> = self
            .under("data\\")
            .filter_map(|path| {
                let mut names = path.names().skip(1);
                let language = names.next()?;
                (names.next() == Some("art")).then_some(language)
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

    /// The INI files of `name`, in the order the game loads them, as `Object` or `Terrain`: for
    /// each of `Data\INI\Default\<name>` and `Data\INI\<name>`, the file of that name, then the
    /// files in that directory, then those in its directories below.
    pub(crate) fn inis(&self, name: &str) -> Vec<ArchivePath> {
        let mut inis = Vec::new();
        for base in [
            format!("data\\ini\\default\\{name}"),
            format!("data\\ini\\{name}"),
        ] {
            let base = ArchivePath::of(&base);
            let file = ArchivePath::of(&format!("{base}.ini"));
            if self.contains(&file) {
                inis.push(file);
            }
            let depth = base.names().count();
            let base_dir = format!("{base}\\");
            let (direct, deeper): (Vec<&ArchivePath>, Vec<_>) = self
                .under(&base_dir)
                .filter(|path| path.extension() == Some("ini"))
                .partition(|path| path.names().count() == depth + 1);
            inis.extend(direct.into_iter().cloned());
            inis.extend(deeper.into_iter().cloned());
        }
        inis
    }

    /// Each archive's path and SHA-256, in the load order, each of the file of the stamp its
    /// entries were read with, so the bytes a version check knows are the bytes the import reads.
    /// The archives hash on every core, each on one, as the install's gigabytes would take one
    /// core most of a check's run.
    pub(crate) fn hashed(&self) -> Result<Vec<HashedArchive>, ZeroHourError> {
        Parallel::map_job(Job::Import, self.archives.len(), |at| {
            self.archives[at].hashed()
        })
        .into_iter()
        .collect()
    }

    /// The archives under `root`, whose entries are `entries`: each file, or link to one, whose
    /// name ends in `.big`, ignoring case, as the game's search finds them, by its path in the
    /// install, a link by its target's path, as a read follows no link. A name that is not UTF-8
    /// sorts by its lossy text, and opens by its own.
    fn find(root: PathBuf, entries: Vec<DirEntry>) -> Result<Vec<Found>, ZeroHourError> {
        let mut found = Vec::new();
        let mut dirs = vec![(root, String::new(), entries)];
        while let Some((dir, at, entries)) = dirs.pop() {
            for entry in entries {
                let name = format!("{at}{}", entry.name.to_string_lossy());
                let path = dir.join(&entry.name);
                let big = Path::new(&entry.name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("big"));
                match entry.kind {
                    EntryKind::Dir => {
                        let entries = DirEntries::read(&path)?;
                        dirs.push((path, format!("{name}/"), entries));
                    }
                    EntryKind::File if big => found.push(Found { name, path }),
                    EntryKind::Link if big => {
                        let path = DirEntries::link_target(&path)?;
                        found.push(Found { name, path });
                    }
                    EntryKind::File | EntryKind::Link | EntryKind::Other => {}
                }
            }
        }
        Ok(found)
    }
}

impl Archive {
    /// Its name and the SHA-256 of its file, read a chunk at a time into one buffer; then
    /// `ArchiveChanged` when another program changed it since its entries were read.
    fn hashed(&self) -> Result<HashedArchive, ZeroHourError> {
        let mut hasher = ContentHasher::<ContentHashMeaning>::new();
        let len = self.file.len();
        let mut chunk = vec![0; usize::try_from(len.min(HASH_CHUNK)).expect("a chunk fits usize")];
        let mut at = 0;
        while at < len {
            let read = (len - at).min(HASH_CHUNK);
            let bytes = &mut chunk[..usize::try_from(read).expect("a chunk fits usize")];
            self.file.read_at_into(at, bytes)?;
            hasher.update(bytes);
            at += read;
        }
        if !self.file.unchanged()? {
            return Err(ZeroHourError::ArchiveChanged {
                archive: self.file.path().to_owned(),
            });
        }
        Ok(HashedArchive {
            name: self.name.clone(),
            sha256: hasher.finish(),
        })
    }

    /// The `len` bytes from `offset` of its file; `ArchiveChanged` when another program changed
    /// it since its entries were read.
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, ZeroHourError> {
        let bytes = self.file.read_at(offset, len)?;
        if self.file.unchanged()? {
            Ok(bytes)
        } else {
            Err(ZeroHourError::ArchiveChanged {
                archive: self.file.path().to_owned(),
            })
        }
    }
}

#[cfg(test)]
pub(crate) mod samples {
    use campfire_store::internals::TestDir;

    use crate::texture::samples::textures;
    use crate::zero_hour::animation_import::samples::animation_files;
    use crate::zero_hour::format::big::big_archive::samples::big;
    use crate::zero_hour::format::csf::string_table::samples::strings;
    use crate::zero_hour::format::ini::armor_ini::samples::ARMORS;
    use crate::zero_hour::format::ini::command_button_ini::samples::COMMAND_BUTTONS;
    use crate::zero_hour::format::ini::command_set_ini::samples::COMMAND_SETS;
    use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
    use crate::zero_hour::format::ini::multiplayer_ini::samples::MULTIPLAYER;
    use crate::zero_hour::format::ini::object_ini::samples::{
        DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS,
    };
    use crate::zero_hour::format::ini::player_template_ini::samples::PLAYER_TEMPLATES;
    use crate::zero_hour::format::ini::upgrade_ini::samples::UPGRADES;
    use crate::zero_hour::format::ini::weapon_ini::samples::WEAPONS;
    use crate::zero_hour::format::map::map_file::samples::map;
    use crate::zero_hour::format::ref_pack::samples::literal;
    use crate::zero_hour::install::Install;
    use crate::zero_hour::model_import::samples::{nothing_drawn, rock, tank};

    /// The fixture's terrain types: the fixture map's `Sand`, whose texture the install holds,
    /// and its `Grass`, whose texture it lacks.
    const TERRAIN_TYPES: &[u8] =
        b"Terrain Sand\n  Texture = TSand.tga\nEnd\nTerrain Grass\n  Texture = Gone.tga\nEnd\n";

    /// The fixture's game data: cliffs stretched, third blends off.
    const GAME_DATA: &[u8] =
        b"GameData\n  AdjustCliffTextures = Yes\n  Use3WayTerrainBlends = 0\nEnd\n";

    /// The fixture's `Mouse.ini`: a press of 167 ms or less that does not drag is a click.
    const MOUSE: &[u8] =
        b"Mouse\n  DragTolerance = 4\n  DragToleranceMS = 167\nEnd\nMouseCursor Normal\nEnd\n";

    /// The fixture map's file, packed by `RefPack` as the game ships most maps.
    pub(crate) fn packed_map() -> Vec<u8> {
        let plain = map(8);
        let len = u32::try_from(plain.len()).unwrap().to_le_bytes();
        [&b"EAR\0"[..], &len, &literal(&plain)].concat()
    }

    /// An install whose archives each hold `shared.ini`: `A.big`, which also holds the fixture map
    /// in its directory, a map outside one, the two textures, a terrain texture, the terrain types,
    /// the game data, the mouse's, the object INI files, the models, `Rock01.w3d` in the
    /// language directory `English` and, unread, outside it, and the animations; `b.big`, `c.BIG`, base Generals'
    /// `ZH_Generals/base.big`, and the duplicate `Data/INI/INIZH.big`, beside a file that is no
    /// archive.
    pub(crate) fn fixture(test_dir: &TestDir) {
        let packed = packed_map();
        let [rock_texture, sign] = textures();
        let [prop, spray] = nothing_drawn();
        let [aim, walk, sway] = animation_files();
        test_dir.write(
            "zh/A.big",
            big(&[
                ("Data\\a.ini", b"A1"),
                ("shared.ini", b"from A"),
                ("Maps\\Fixture Map\\Fixture Map.map", &packed),
                ("Maps\\Stray\\Other.map", b"no map the game lists"),
                ("Art\\Textures\\Rock.dds", &rock_texture),
                ("Art\\Textures\\Sign.tga", &sign),
                ("Art\\Terrain\\TSand.tga", &sign),
                ("Data\\INI\\Terrain.ini", TERRAIN_TYPES),
                ("Data\\INI\\GameData.ini", GAME_DATA),
                ("Data\\INI\\Mouse.ini", MOUSE),
                ("Data\\INI\\Locomotor.ini", LOCOMOTORS.as_bytes()),
                ("Data\\INI\\Weapon.ini", WEAPONS.as_bytes()),
                ("Data\\INI\\Armor.ini", ARMORS.as_bytes()),
                ("Data\\INI\\PlayerTemplate.ini", PLAYER_TEMPLATES.as_bytes()),
                ("Data\\INI\\CommandSet.ini", COMMAND_SETS.as_bytes()),
                ("Data\\INI\\CommandButton.ini", COMMAND_BUTTONS.as_bytes()),
                ("Data\\INI\\Upgrade.ini", UPGRADES.as_bytes()),
                ("Data\\INI\\Multiplayer.ini", MULTIPLAYER.as_bytes()),
                ("Data\\English\\generals.csf", &strings()),
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
                ("Art\\W3D\\Tank_Aim.w3d", &aim),
                ("Art\\W3D\\Ghost_Walk.w3d", &walk),
                ("Art\\W3D\\Pole_Sway.w3d", &sway),
            ]),
        );
        test_dir.write(
            "zh/b.big",
            big(&[("shared.ini", b"from b"), ("B\\b.ini", b"b")]),
        );
        test_dir.write("zh/c.BIG", big(&[("c.ini", b"c")]));
        test_dir.write("zh/Data/INI/INIZH.big", big(&[("shared.ini", b"dup")]));
        test_dir.write(
            "zh/ZH_Generals/base.big",
            big(&[("shared.ini", b"base"), ("base.ini", b"base only")]),
        );
        test_dir.write("zh/readme.txt", "no archive");
    }

    /// The fixture install in a test directory of its own, opened; the directory lives as long as
    /// it.
    #[derive(Debug)]
    pub(crate) struct FixtureInstall {
        pub(crate) test_dir: TestDir,
        pub(crate) install: Install,
    }

    impl FixtureInstall {
        pub(crate) fn new() -> Self {
            let test_dir = TestDir::new();
            fixture(&test_dir);
            let install = Install::open(&test_dir.path("zh")).unwrap();
            Self { test_dir, install }
        }
    }
}

#[cfg(test)]
mod tests;
