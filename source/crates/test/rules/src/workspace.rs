use std::collections::BTreeSet;
use std::path::Path;

use campfire_store::internals::{CargoManifest, SourceFile, SourceFiles};
use campfire_store::{DirEntries, DirEntry, EntryKind, InputFile};

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
    let manifest = read(
        source()
            .join("crates")
            .join(dir)
            .join("Cargo.toml")
            .to_str()
            .unwrap(),
    );
    let mut names: Vec<String> = CargoManifest::dependencies(&manifest)
        .into_iter()
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    names
}

/// The dependencies of `manifest` its build requires, not those a feature turns on.
pub(crate) fn required_dependencies(manifest: &str) -> Vec<&str> {
    CargoManifest::section(manifest, "dependencies")
        .filter(|line| !line.contains("optional = true"))
        .map(CargoManifest::dependency)
        .collect()
}

/// The crates of the workspace a match runs on, by name: the release and each crate of the
/// workspace its required dependencies reach, of `manifests`, the workspace's.
pub(crate) fn sim_closure(manifests: &[(String, String)]) -> BTreeSet<String> {
    let manifest = |name: &str| {
        let (_, manifest) = (manifests.iter())
            .find(|(crate_name, _)| crate_name == name)
            .expect("a walk reaches the workspace's crates");
        manifest
    };
    let workspace = |name: &str| manifests.iter().any(|(crate_name, _)| crate_name == name);
    let mut reached = BTreeSet::from(["campfire-capabilities".to_owned()]);
    let mut next = vec!["campfire-capabilities".to_owned()];
    while let Some(at) = next.pop() {
        for other in required_dependencies(manifest(&at)) {
            if workspace(other) && reached.insert(other.to_owned()) {
                next.push(other.to_owned());
            }
        }
    }
    reached
}

/// A crate of the workspace: its directory's name, its package's name, and its manifest's text.
#[derive(Debug)]
pub(crate) struct Crate {
    pub(crate) dir_name: String,
    /// Its directory's path under `source/`, as `sources` names a file's.
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) manifest: String,
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
            let manifest = read(dir.join("Cargo.toml").to_str().unwrap());
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
                name: CargoManifest::package_name(&manifest).to_owned(),
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

/// The crates of the workspace, each by its package's name, with the text of its manifest.
pub(crate) fn manifests() -> Vec<(String, String)> {
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
        for file in SourceFiles::under(&source().join(dir_name)) {
            read.push(SourceFile {
                path: format!("{dir_name}/{}", file.path),
                ..file
            });
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
