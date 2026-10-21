use std::collections::BTreeSet;
use std::path::Path;

use campfire_store::{DirEntries, DirEntry, EntryKind, InputFile};

use crate::manifest::{CargoDependency, CargoManifest};
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
    let manifest = CargoManifest::parse(&read(path.to_str().unwrap()));
    let mut names: Vec<String> = manifest
        .dependencies()
        .map(|(name, _)| name.to_owned())
        .collect();
    names.sort_unstable();
    names
}

/// The crates of the workspace a match runs on, by name: the release and each crate of the
/// workspace its required dependencies reach, of `manifests`, the workspace's.
pub(crate) fn sim_closure(manifests: &[(String, CargoManifest)]) -> BTreeSet<String> {
    let workspace = |name: &str| manifests.iter().any(|(crate_name, _)| crate_name == name);
    let mut reached = required_closure(manifests, "campfire-capabilities");
    reached.retain(|name| workspace(name));
    reached.insert("campfire-capabilities".to_owned());
    reached
}

/// Every crate the build of `name`, a crate of `manifests`, requires, by name: its required
/// dependencies, and theirs through each crate of the workspace, as a dependency from outside it
/// names its own in no manifest here.
pub(crate) fn required_closure(
    manifests: &[(String, CargoManifest)],
    name: &str,
) -> BTreeSet<String> {
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

/// Whether a build follows the dev-dependencies of the crate it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DevEdges {
    /// Its dependencies alone, as its test build holds them besides its dev-dependencies.
    Left,
    /// Its dependencies and its dev-dependencies: the whole of its tests' build.
    Followed,
}

/// Every crate the test build of `name`, a crate of `manifests`, holds but itself: its
/// dependencies with its own `internals` on, each feature its dependencies and dev-dependencies
/// turn on them, and, past them, each dependency of a crate of the workspace with the features its
/// edge turns on; a crate outside the workspace is reached as a manifest here names it. With
/// `DevEdges::Followed`, its dev-dependencies too.
pub(crate) fn test_build<'a>(
    manifests: &'a [(String, CargoManifest)],
    name: &str,
    dev_edges: DevEdges,
) -> BTreeSet<&'a str> {
    let (root, manifest) = manifests
        .iter()
        .find(|(crate_name, _)| crate_name == name)
        .expect("a test build starts at a crate of the workspace");
    let mut build = FeatureBuild {
        manifests,
        crates: BTreeSet::new(),
        features: BTreeSet::new(),
        enabled: BTreeSet::new(),
        waiting: Vec::new(),
        next: vec![Step::Crate(root)],
    };
    if manifest.feature("internals").is_some() {
        build.next.push(Step::Feature(root, "internals"));
    }
    for (other, dependency) in manifest.dev_dependencies() {
        let normal = manifest.dependencies().any(|(name, _)| name == other);
        if normal || dev_edges == DevEdges::Followed {
            build.edge(other, dependency);
        }
    }
    build.run();
    build.crates.remove(root.as_str());
    build.crates
}

/// A build's walk of the crates and features it turns on, from the steps it has yet to take.
#[derive(Debug)]
struct FeatureBuild<'a> {
    manifests: &'a [(String, CargoManifest)],
    crates: BTreeSet<&'a str>,
    features: BTreeSet<(&'a str, &'a str)>,
    /// Each optional dependency a crate's features turned on: the crate, and the dependency.
    enabled: BTreeSet<(&'a str, &'a str)>,
    /// Each weak entry, `x?/f`, of an optional `x` that no feature turned on yet: the crate, `x`,
    /// and `f`.
    waiting: Vec<(&'a str, &'a str, &'a str)>,
    next: Vec<Step<'a>>,
}

/// One step of a build: a crate it holds, or a feature it turns on in a crate.
#[derive(Debug, Clone, Copy)]
enum Step<'a> {
    Crate(&'a str),
    Feature(&'a str, &'a str),
}

impl<'a> FeatureBuild<'a> {
    fn manifest(&self, name: &str) -> Option<&'a CargoManifest> {
        let manifests = self.manifests;
        manifests
            .iter()
            .find(|(crate_name, _)| crate_name == name)
            .map(|(_, manifest)| manifest)
    }

    fn edge(&mut self, other: &'a str, dependency: &'a CargoDependency) {
        self.next.push(Step::Crate(other));
        for feature in &dependency.features {
            self.next.push(Step::Feature(other, feature));
        }
    }

    /// Holds `other`, a dependency of `name` through `dependency`, which a feature of `name` turns
    /// on: an optional one holds from now on, with each weak entry of `name` that waited for it.
    fn turn_on_dependency(
        &mut self,
        name: &'a str,
        other: &'a str,
        dependency: &'a CargoDependency,
    ) {
        self.edge(other, dependency);
        if dependency.optional && self.enabled.insert((name, other)) {
            let waited = (self.waiting.iter()).filter(|&&(of, on, _)| of == name && on == other);
            self.next
                .extend(waited.map(|&(_, _, inner)| Step::Feature(other, inner)));
        }
    }

    fn run(&mut self) {
        while let Some(step) = self.next.pop() {
            match step {
                Step::Crate(name) => self.hold(name),
                Step::Feature(name, feature) => self.turn_on(name, feature),
            }
        }
    }

    /// Holds the crate `name` and each dependency its build requires.
    fn hold(&mut self, name: &'a str) {
        if !self.crates.insert(name) {
            return;
        }
        let Some(manifest) = self.manifest(name) else {
            return;
        };
        for (other, dependency) in manifest.dependencies() {
            if !dependency.optional {
                self.edge(other, dependency);
            }
        }
    }

    /// Turns on `feature` of the crate `name`: each entry of the feature, or the optional
    /// dependency of that name. A weak entry, `x?/f`, turns `f` on of a required `x`, and of an
    /// optional one once a feature of `name` turns it on, as Cargo's does.
    fn turn_on(&mut self, name: &'a str, feature: &'a str) {
        if !self.features.insert((name, feature)) {
            return;
        }
        self.next.push(Step::Crate(name));
        let Some(manifest) = self.manifest(name) else {
            return;
        };
        let dependency = |other: &str| {
            manifest
                .dependencies()
                .find(|(dependency_name, _)| *dependency_name == other)
        };
        let Some(entries) = manifest.feature(feature) else {
            if let Some((other, dependency)) = dependency(feature) {
                self.turn_on_dependency(name, other, dependency);
            }
            return;
        };
        for entry in entries {
            if let Some(other) = entry.strip_prefix("dep:") {
                if let Some((other, dependency)) = dependency(other) {
                    self.turn_on_dependency(name, other, dependency);
                }
            } else if let Some((other, inner)) = entry.split_once('/') {
                let weak = other.ends_with('?');
                let Some((other, dependency)) = dependency(other.trim_end_matches('?')) else {
                    continue;
                };
                if !weak {
                    self.turn_on_dependency(name, other, dependency);
                } else if dependency.optional && !self.enabled.contains(&(name, other)) {
                    self.waiting.push((name, other, inner));
                    continue;
                }
                self.next.push(Step::Feature(other, inner));
            } else {
                self.next.push(Step::Feature(name, entry));
            }
        }
    }
}

/// A crate of the workspace: its directory's name, its package's name, and its manifest.
#[derive(Debug)]
pub(crate) struct Crate {
    pub(crate) dir_name: String,
    /// Its directory's path under `source/`, as `sources` names a file's.
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) manifest: CargoManifest,
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
            let manifest = CargoManifest::parse(&read(dir.join("Cargo.toml").to_str().unwrap()));
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
pub(crate) fn manifests() -> Vec<(String, CargoManifest)> {
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

/// Each Rust source file of `crates`, by its path under `source/`, with its text: the walk of
/// their directories alone, for a rule that reads a few crates of the workspace.
pub(crate) fn sources_of<'a>(crates: impl IntoIterator<Item = &'a Crate>) -> Vec<SourceFile> {
    let mut read = Vec::new();
    for krate in crates {
        for mut file in SourceFiles::under(&source().join(&krate.path)) {
            file.path = format!("{}/{}", krate.path, file.path);
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
