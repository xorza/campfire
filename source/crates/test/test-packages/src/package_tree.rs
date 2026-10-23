use campfire_mode::{ModeData, PlayersData, SavesData};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::{Arc, OnceLock};

use campfire_codec::Toml;
use campfire_codec::internals::TomlDocument;
use campfire_package::{LoadError, ModePackages};
use campfire_package_disk::PackageDir;
use campfire_package_disk::internals::TreeDir;
use campfire_store::{DirEntries, EntryKind, InputFile};

use crate::test_mode::TestMode;

/// The most bytes a test reads of a workspace package's file: far past any the test packages
/// hold.
const FILE_LEN: usize = 64 << 20;

/// The tree of a test mode, every file in memory by its path from the tree's root, as a test
/// edits it before it loads the mode. The first load after an edit indexes every package of the
/// tree again, and the loads after it share that tree.
#[derive(Debug, Clone)]
pub struct PackageTree {
    mode: TestMode,
    files: BTreeMap<PathBuf, Vec<u8>>,
    /// The files with each package's index, once a load built them since the last edit.
    indexed: OnceLock<Arc<BTreeMap<PathBuf, Vec<u8>>>>,
}

/// An edit of one file of a `PackageTree`: text added after its end on a line of its own, text
/// put in place of the first of another, the text of a new file, or, by the path of a TOML key, a
/// value set or added, a key removed, or a key renamed. A path holds keys and array indices
/// joined by dots, so an edit by path holds whatever value the file has there.
#[derive(Debug, Clone, Copy)]
pub enum Edit<'a> {
    Append(&'a str),
    Replace(&'a str, &'a str),
    Create(&'a str),
    /// The value at the path, as TOML writes it.
    Set(&'a str, &'a str),
    Remove(&'a str),
    /// The key at the path, given the name.
    Rename(&'a str, &'a str),
}

impl PackageTree {
    /// The tree of `mode`, read from disk.
    pub fn of(mode: TestMode) -> Self {
        let mut files = BTreeMap::new();
        Self::read(
            &PackageDir::workspace(mode.tree()),
            Path::new(""),
            &mut files,
        );
        Self {
            mode,
            files,
            indexed: OnceLock::new(),
        }
    }

    /// The text of `file`, by its path from the tree's root.
    pub fn text(&self, file: &str) -> &str {
        let bytes = self
            .files
            .get(Path::new(file))
            .unwrap_or_else(|| panic!("the tree has no {file}"));
        str::from_utf8(bytes).expect("a test package's text is UTF-8")
    }

    /// Makes `edit` to `file`, by its path from the tree's root.
    pub fn edit(&mut self, file: &str, edit: Edit<'_>) -> &mut Self {
        let edited = match edit {
            Edit::Create(text) => text.to_owned(),
            Edit::Append(more) => format!("{}\n{more}", self.text(file)),
            Edit::Replace(from, to) => {
                let text = self.text(file);
                assert!(text.contains(from), "{file}: {from}");
                text.replacen(from, to, 1)
            }
            Edit::Set(at, value) => self.edited(file, |document| document.set(at, value)),
            Edit::Remove(at) => self.edited(file, |document| document.remove(at)),
            Edit::Rename(at, name) => self.edited(file, |document| document.rename(at, name)),
        };
        self.files.insert(PathBuf::from(file), edited.into_bytes());
        self.indexed = OnceLock::new();
        self
    }

    /// Makes the mode's `[players]` as `players` says, in place of its own, and its `[saves]` as
    /// `saves` says.
    pub fn players_and_saves(&mut self, players: &PlayersData, saves: &SavesData) -> &mut Self {
        let data = format!("{}/data/mode.toml", self.mode.root());
        let added = format!(
            "\n[players]\n{}\n[saves]\n{}",
            Toml::write(players),
            Toml::write(saves)
        );
        self.edit(&data, Edit::Remove(ModeData::PLAYERS));
        self.edit(&data, Edit::Append(&added))
    }

    /// Makes each of `edits`, in order, each to a file by its path from the tree's root.
    pub fn edit_all<'a>(
        &mut self,
        edits: impl IntoIterator<Item = (&'a str, Edit<'a>)>,
    ) -> &mut Self {
        for (file, edit) in edits {
            self.edit(file, edit);
        }
        self
    }

    /// Puts `bytes` at `file`, by its path from the tree's root: a binary file, which no text edit
    /// writes, new or in place of the one there.
    pub fn put(&mut self, file: &str, bytes: Vec<u8>) -> &mut Self {
        self.files.insert(PathBuf::from(file), bytes);
        self.indexed = OnceLock::new();
        self
    }

    /// The package at `root`, by its path from the tree's root, in memory.
    pub fn dir(&self, root: &str) -> TreeDir {
        let tree = self
            .indexed
            .get_or_init(|| Arc::new(TreeDir::reindexed(self.files.clone())));
        TreeDir::new(Arc::clone(tree), root)
    }

    /// The tree's mode on its map, or why it does not load.
    pub fn load(&self) -> Result<ModePackages, LoadError> {
        ModePackages::at(&self.dir(self.mode.root()), Some(&self.mode.map()))
    }

    /// The tree's mode on its map.
    pub fn packages(&self) -> ModePackages {
        self.load().expect("the edited test mode loads")
    }

    /// The text of `file` as TOML, with `edit` made to it.
    fn edited(&self, file: &str, edit: impl FnOnce(&mut TomlDocument)) -> String {
        let mut document = TomlDocument::parse(self.text(file));
        edit(&mut document);
        document.text()
    }

    fn read(dir: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in DirEntries::read(dir).unwrap() {
            let (on_disk, path) = (dir.join(&entry.name), at.join(&entry.name));
            if entry.kind == EntryKind::Dir {
                Self::read(&on_disk, &path, files);
            } else {
                files.insert(path, InputFile::read(&on_disk, FILE_LEN).unwrap());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The packages of `dir`, as two loads compare them.
    fn seen(dir: &TreeDir) -> String {
        let packages = ModePackages::at(dir, Some(&TestMode::Moba3v3.map())).unwrap();
        let views: Vec<_> = packages
            .packages()
            .map(|view| format!("{:?}", view.content))
            .collect();
        format!(
            "{:?}\n{:?}\n{:?}\n{views:?}",
            packages.manifest(),
            packages.data(),
            packages.map()
        )
    }

    #[test]
    fn an_edit_by_path_changes_nothing_but_its_value() {
        // Every TOML file written back unchanged loads the same packages: the sorted keys and the
        // lost comments and layout mean nothing.
        let read = PackageTree::of(TestMode::Moba3v3);
        let mut written = read.clone();
        for (path, bytes) in &mut written.files {
            if path
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                let text = str::from_utf8(bytes).unwrap();
                *bytes = TomlDocument::parse(text).text().into_bytes();
            }
        }
        let root = TestMode::Moba3v3.root();
        assert_eq!(seen(&read.dir(root)), seen(&written.dir(root)));
    }

    #[test]
    fn each_edit_changes_its_file_as_it_says() {
        // A tree of one file; each edit, made to a fresh copy, and the text it leaves.
        let tree = PackageTree {
            mode: TestMode::Lane,
            files: BTreeMap::from([("a.toml".into(), b"[t]\nx = 1\ny = [2, 3]\n".to_vec())]),
            indexed: OnceLock::new(),
        };
        let cases = [
            (Edit::Append("z = 4\n"), "[t]\nx = 1\ny = [2, 3]\n\nz = 4\n"),
            (Edit::Replace("x = 1", "x = 5"), "[t]\nx = 5\ny = [2, 3]\n"),
            (Edit::Create("w = 0\n"), "w = 0\n"),
            (Edit::Set("t.y.1", "7"), "[t]\nx = 1\ny = [2, 7]\n"),
            (
                Edit::Set("u.v", "true"),
                "[t]\nx = 1\ny = [2, 3]\n\n[u]\nv = true\n",
            ),
            (Edit::Remove("t.x"), "[t]\ny = [2, 3]\n"),
            (Edit::Remove("t.y.0"), "[t]\nx = 1\ny = [3]\n"),
            (Edit::Rename("t.x", "z"), "[t]\ny = [2, 3]\nz = 1\n"),
        ];
        for (edit, expected) in cases {
            let mut edited = tree.clone();
            edited.edit("a.toml", edit);
            assert_eq!(edited.text("a.toml"), expected, "{edit:?}");
        }
    }
}
