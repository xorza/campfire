//! The reference packages, read from disk once, and in memory with edits made.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use campfire_package::PackageDir;

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
        let text = match edit {
            Edit::Replace(from, to) => {
                let text = String::from_utf8(files[&path].clone()).unwrap();
                assert!(text.contains(from), "{file}: {from}");
                text.replacen(from, to, 1)
            }
            Edit::Create(text) => text.to_owned(),
        };
        files.insert(path, text.into_bytes());
    }
    PackageDir::in_memory(Arc::new(files), root)
}

/// Text put in place of the first of another, or the text of a new file.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Edit {
    Replace(&'static str, &'static str),
    Create(&'static str),
}
