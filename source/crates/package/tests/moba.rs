//! The reference packages, read from disk once, and in memory with edits made.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use campfire_package::{ModePackages, PackageDir};
use toml::{Table, Value};

pub(crate) fn moba() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba"))
}

/// Every file of the reference packages, by its path from their root, read from disk once.
pub(crate) fn moba_files() -> &'static BTreeMap<PathBuf, Vec<u8>> {
    static FILES: OnceLock<BTreeMap<PathBuf, Vec<u8>>> = OnceLock::new();
    FILES.get_or_init(|| {
        let mut files = BTreeMap::new();
        read_tree(&moba(), Path::new(""), &mut files);
        files
    })
}

fn read_tree(dir: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = at.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            read_tree(&entry.path(), &path, files);
        } else {
            files.insert(path, fs::read(entry.path()).unwrap());
        }
    }
}

/// The reference packages in memory with `edits` made, each to a file by its path from their
/// root: their 3v3.
pub(crate) fn edited<'a>(edits: impl IntoIterator<Item = (&'a str, Edit)>) -> PackageDir {
    edited_at("modes/3v3", edits)
}

/// The reference packages in memory with `edits` made, each to a file by its path from their
/// root: the package at `root`.
pub(crate) fn edited_at<'a>(
    root: &str,
    edits: impl IntoIterator<Item = (&'a str, Edit)>,
) -> PackageDir {
    let mut files = moba_files().clone();
    for (file, edit) in edits {
        let path = PathBuf::from(file);
        let text = || String::from_utf8(files[&path].clone()).unwrap();
        let text = match edit {
            Edit::Replace(from, to) => {
                let text = text();
                assert!(text.contains(from), "{file}: {from}");
                text.replacen(from, to, 1)
            }
            Edit::Create(text) => text.to_owned(),
            Edit::Set(at, value) => {
                let value = toml::from_str::<Table>(&format!("value = {value}")).unwrap();
                let value = value["value"].clone();
                at_path(&text(), at, Missing::Add, |parent, key| match parent {
                    Value::Table(table) => drop(table.insert(key.to_owned(), value)),
                    Value::Array(array) => array[index(key)] = value,
                    _ => panic!("{file}: {at} is inside a value"),
                })
            }
            Edit::Remove(at) => at_path(&text(), at, Missing::Fail, |parent, key| match parent {
                Value::Table(table) => drop(table.remove(key).expect("a key to remove")),
                Value::Array(array) => drop(array.remove(index(key))),
                _ => panic!("{file}: {at} is inside a value"),
            }),
            Edit::Rename(at, name) => at_path(&text(), at, Missing::Fail, |parent, key| {
                let Value::Table(table) = parent else {
                    panic!("{file}: {at} is no key");
                };
                let value = table.remove(key).expect("a key to rename");
                table.insert(name.to_owned(), value);
            }),
        };
        files.insert(path, text.into_bytes());
    }
    PackageDir::in_memory(Arc::new(files), root)
}

fn index(key: &str) -> usize {
    key.parse()
        .unwrap_or_else(|_| panic!("{key} indexes an array"))
}

/// What a path does at a table that lacks its next key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Missing {
    /// Adds the key, an empty table.
    Add,
    Fail,
}

/// `text` as TOML with `edit` made to the value that holds the last key of `path`, its keys and
/// array indices joined by dots, and written back. The keys of each table come back sorted, which
/// no package file depends on.
fn at_path(
    text: &str,
    path: &str,
    missing: Missing,
    edit: impl FnOnce(&mut Value, &str),
) -> String {
    let mut root = Value::Table(toml::from_str::<Table>(text).unwrap());
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let parent = parent
        .split('.')
        .filter(|key| !key.is_empty())
        .fold(&mut root, |value, key| match value {
            Value::Table(table) => match missing {
                Missing::Add => table
                    .entry(key)
                    .or_insert_with(|| Value::Table(Table::new())),
                Missing::Fail => table
                    .get_mut(key)
                    .unwrap_or_else(|| panic!("no {key} on the way to {path}")),
            },
            Value::Array(array) => &mut array[index(key)],
            _ => panic!("{path} passes through a value"),
        });
    edit(parent, key);
    toml::to_string(&root).unwrap()
}

/// An edit to a file: text put in place of the first of another, the text of a new file, or, by
/// the path of a TOML key, a value set or added, a key removed, or a key renamed. A path holds
/// keys and array indices joined by dots, so an edit by path holds whatever value the file has
/// there.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Edit {
    Replace(&'static str, &'static str),
    Create(&'static str),
    /// The value at the path, as TOML writes it.
    Set(&'static str, &'static str),
    Remove(&'static str),
    /// The key at the path, given the name.
    Rename(&'static str, &'static str),
}

#[test]
fn an_edit_by_path_changes_nothing_but_its_value() {
    // Every TOML file written back unchanged loads the same packages: the sorted keys and the
    // lost comments and layout mean nothing.
    let mut files = moba_files().clone();
    for (path, bytes) in &mut files {
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            let text = String::from_utf8(bytes.clone()).unwrap();
            *bytes = at_path(&text, "", Missing::Fail, |_, _| {}).into_bytes();
        }
    }
    let written = PackageDir::in_memory(Arc::new(files), "modes/3v3");
    let [read, written] = [edited([]), written].map(|dir| {
        let packages = ModePackages::from_package_dir(&dir).unwrap();
        let views: Vec<_> = packages
            .packages()
            .map(|view| format!("{:?}", view.content))
            .collect();
        (
            format!("{:?}", packages.manifest()),
            format!("{:?}", packages.data()),
            format!("{:?}", packages.map()),
            views,
        )
    });
    assert_eq!(read, written);
}
