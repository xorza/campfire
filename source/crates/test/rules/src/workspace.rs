use std::collections::BTreeSet;
use std::path::Path;

use campfire_store::{DirEntries, DirEntry, EntryKind, InputFile};

use crate::manifest::Manifest;
use crate::source_file::SourceFile;
use crate::source_files::SourceFiles;

/// The most bytes a test reads of a file of the workspace's own: far past any it holds.
const FILE_LEN: usize = 1 << 20;

/// The text of the workspace's file at `path`.
pub(crate) fn read(path: &str) -> String {
    InputFile::read_text(Path::new(path), FILE_LEN).unwrap()
}

/// The workspace's `source/` directory, three levels over this crate's manifest:
/// `crates/test/rules`.
pub(crate) fn source() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}

/// The dependencies of the crate at `dir`, under `source/crates/`, sorted.
pub(crate) fn crate_dependencies(dir: &str) -> Vec<String> {
    let path = source().join("crates").join(dir).join("Cargo.toml");
    let manifest = Manifest::parse(&read(path.to_str().unwrap()));
    let mut names: Vec<String> = manifest
        .dependencies()
        .map(|(name, _)| name.to_owned())
        .collect();
    names.sort_unstable();
    names
}

/// The crates of the workspace a match runs on, by name: the release and each crate of the
/// workspace its required dependencies reach, of `manifests`, the workspace's.
pub(crate) fn sim_closure(manifests: &[(String, Manifest)]) -> BTreeSet<String> {
    let workspace = |name: &str| manifests.iter().any(|(crate_name, _)| crate_name == name);
    let mut reached = required_closure(manifests, "campfire-capabilities");
    reached.retain(|name| workspace(name));
    reached.insert("campfire-capabilities".to_owned());
    reached
}

/// Every crate the build of `name`, a crate of `manifests`, requires, by name: its required
/// dependencies, and theirs through each crate of the workspace, as a dependency from outside it
/// names its own in no manifest here.
pub(crate) fn required_closure(manifests: &[(String, Manifest)], name: &str) -> BTreeSet<String> {
    let manifest = |name: &str| {
        manifests
            .iter()
            .find(|(crate_name, _)| crate_name == name)
            .map(|(_, manifest)| manifest)
    };
    let mut reached = BTreeSet::new();
    let mut next = vec![name.to_owned()];
    while let Some(at) = next.pop() {
        let Some(manifest) = manifest(&at) else {
            continue;
        };
        for other in manifest.required() {
            if reached.insert(other.to_owned()) {
                next.push(other.to_owned());
            }
        }
    }
    reached
}

/// A crate of the workspace: its directory's name, its package's name, and its manifest.
#[derive(Debug)]
pub(crate) struct Crate {
    pub(crate) dir_name: String,
    /// Its directory's path under `source/`, as `sources` names a file's.
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) manifest: Manifest,
}

/// The crates of the workspace, by their packages' names: each directory of `crates/` and `checks/`
/// that holds a `Cargo.toml`, at any depth of the groups; the walk goes on into a crate's
/// directory, so a test finds a crate that one holds.
pub(crate) fn crates() -> Vec<Crate> {
    let mut found = Vec::new();
    let mut dirs = vec![source().join("crates"), source().join("checks")];
    while let Some(dir) = dirs.pop() {
        let entries = DirEntries::read(&dir).unwrap();
        if entries
            .iter()
            .any(|inner: &DirEntry| inner.name == "Cargo.toml")
        {
            let manifest = Manifest::parse(&read(dir.join("Cargo.toml").to_str().unwrap()));
            // Joined by `/` on every OS, as a source file's path is.
            let path: Vec<&str> = dir
                .strip_prefix(source())
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str().unwrap())
                .collect();
            found.push(Crate {
                dir_name: dir.file_name().unwrap().to_str().unwrap().to_owned(),
                path: path.join("/"),
                name: manifest.name().to_owned(),
                manifest,
            });
        }
        let own = ["src", "tests", "benches", "examples"];
        dirs.extend(
            entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Dir)
                .filter(|entry| !own.iter().any(|name| entry.name == *name))
                .map(|entry| dir.join(&entry.name)),
        );
    }
    found.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The crates of the workspace, each by its package's name, with its manifest.
pub(crate) fn manifests() -> Vec<(String, Manifest)> {
    crates()
        .into_iter()
        .map(|krate| (krate.name, krate.manifest))
        .collect()
}

/// Each Rust source file of the workspace's crates and checks, by its path under `source/`,
/// with its text.
pub(crate) fn sources() -> Vec<SourceFile> {
    let mut read = Vec::new();
    for dir_name in ["crates", "checks"] {
        for mut file in SourceFiles::under(&source().join(dir_name)) {
            file.path = format!("{dir_name}/{}", file.path);
            read.push(file);
        }
    }
    read
}

/// The index in `crates` of the crate that holds the source file at `path`, under `source/`: the
/// deepest whose directory holds it.
pub(crate) fn holder(crates: &[Crate], path: &str) -> Option<usize> {
    crates
        .iter()
        .enumerate()
        .filter(|(_, krate)| path.starts_with(&format!("{}/", krate.path)))
        .max_by_key(|(_, krate)| krate.path.len())
        .map(|(at, _)| at)
}
