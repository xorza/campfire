use std::path::Path;

use campfire_store::internals::{SourceFile, SourceFiles};
use campfire_store::{DirEntries, DirEntry, EntryKind, InputFile};

/// The most bytes a test reads of a file of the workspace's own: far past any it holds.
const FILE_LEN: usize = 1 << 20;

/// The text of the workspace's file at `path`.
pub(crate) fn read(path: &str) -> String {
    InputFile::read_text(Path::new(path), FILE_LEN).unwrap()
}

/// The names of the crates the manifest `manifest` depends on, its target-specific dependencies
/// among them, in the order it lists them. Read by hand, as the crate has no TOML parser: only the
/// known sections may appear, so a dependency in another form, such as `[dependencies.name]` or a
/// dotted key before the first section, fails instead of passing unseen.
pub(crate) fn dependencies(manifest: &str) -> Vec<&str> {
    let mut section = "";
    let mut names = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            section = header.strip_suffix(']').unwrap();
            let target =
                section.starts_with("target.'cfg(") && section.ends_with(")'.dependencies");
            let known = [
                "package",
                "features",
                "dependencies",
                "dev-dependencies",
                "[test]",
                "lints",
            ];
            assert!(known.contains(&section) || target, "section [{section}]");
            if target {
                section = "dependencies";
            }
        } else {
            assert!(!section.is_empty(), "{line:?} before the first section");
            if section == "dependencies" {
                names.push(dependency(line));
            }
        }
    }
    names
}

/// The workspace's `source/` folder, three levels over this crate's manifest: `crates/test/rules`.
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
    let mut names: Vec<String> = dependencies(&manifest)
        .into_iter()
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    names
}

/// A crate of the workspace: its folder's name, its package's name, and its manifest's text.
#[derive(Debug)]
pub(crate) struct Crate {
    pub(crate) folder: String,
    /// Its folder's path under `source/`, as `sources` names a file's.
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) manifest: String,
}

/// The crates of the workspace, by their packages' names: each folder of `crates/` and `checks/`
/// that holds a `Cargo.toml`, at any depth of the groups, as a crate's folder may hold other
/// crates' folders beside its own `src/`.
pub(crate) fn crates() -> Vec<Crate> {
    let mut found = Vec::new();
    let mut folders = vec![source().join("crates"), source().join("checks")];
    while let Some(dir) = folders.pop() {
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
                folder: dir.file_name().unwrap().to_str().unwrap().to_owned(),
                path: path.join("/"),
                name: package_name(&manifest).to_owned(),
                manifest,
            });
        }
        let own = ["src", "tests", "benches", "examples"];
        folders.extend(
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

/// The package's name the manifest `manifest` gives.
fn package_name(manifest: &str) -> &str {
    let line = section(manifest, "package")
        .find(|line| line.starts_with("name ") || line.starts_with("name="))
        .expect("a manifest names its package");
    line.split('"')
        .nth(1)
        .expect("a package's name is a string")
}

/// The lines of `manifest`'s section `[name]`, past its header.
pub(crate) fn section<'a>(manifest: &'a str, name: &str) -> impl Iterator<Item = &'a str> {
    let header = format!("[{name}]");
    manifest
        .lines()
        .map(str::trim)
        .skip_while(move |line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// The crate a line of a manifest's dependencies names.
pub(crate) fn dependency(line: &str) -> &str {
    line.split(['.', ' ', '=']).next().unwrap()
}

/// Each Rust source file of the workspace's crates and checks, by its path under `source/`,
/// with its text.
pub(crate) fn sources() -> Vec<SourceFile> {
    let mut read = Vec::new();
    for folder in ["crates", "checks"] {
        for file in SourceFiles::under(&source().join(folder)) {
            read.push(SourceFile {
                path: format!("{folder}/{}", file.path),
                ..file
            });
        }
    }
    read
}

/// The index in `crates` of the crate that holds the source file at `path`, under `source/`: the
/// deepest whose folder holds it.
pub(crate) fn holder(crates: &[Crate], path: &str) -> Option<usize> {
    crates
        .iter()
        .enumerate()
        .filter(|(_, krate)| path.starts_with(&format!("{}/", krate.path)))
        .max_by_key(|(_, krate)| krate.path.len())
        .map(|(at, _)| at)
}
