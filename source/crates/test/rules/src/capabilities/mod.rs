//! The rules of the capability crates' code: where their sets are named, their roles' and books'
//! names, their lookups by name, and the words of a genre they leave to data.

use crate::source_file::SourceFile;
use crate::workspace::{Crate, Workspace};

pub(crate) mod genre_words;
pub(crate) mod lookups;
pub(crate) mod part_names;
pub(crate) mod roles;
pub(crate) mod sets;

/// The capability crates' group under `source/`, which holds each capability crate.
const GROUP: &str = "crates/capabilities/";

/// The top crate of the group, whose files go by their paths from its `src`.
const TOP: &str = "crates/capabilities/capabilities";

/// A source file of production code, by its path as these rules name it.
#[derive(Debug)]
struct NamedFile<'a> {
    path: String,
    file: &'a SourceFile,
}

/// How the rules name a crate's files: by their paths from its `src`, or under a name.
#[derive(Debug)]
enum Naming {
    FromSrc,
    Under(String),
}

/// How the rules name the files of `krate`, if it is a capability crate: under its directory's
/// name, `-` as `_`, or from its `src` for the top crate.
fn capability(krate: &Crate) -> Option<Naming> {
    if krate.path == TOP {
        Some(Naming::FromSrc)
    } else {
        in_capability(&krate.path).then(|| Naming::Under(krate.dir_name.replace('-', "_")))
    }
}

/// Whether the file or the crate at `path`, under `source/`, is a capability crate's.
fn in_capability(path: &str) -> bool {
    path.starts_with(GROUP)
}

/// Each capability crate's source of production code, of `workspace`.
fn capability_sources(workspace: &Workspace) -> Vec<NamedFile<'_>> {
    named_files(workspace, capability)
}

/// The sources of production code of each crate of `workspace` that `naming` names, by the path
/// it gives.
fn named_files(
    workspace: &Workspace,
    naming: impl Fn(&Crate) -> Option<Naming>,
) -> Vec<NamedFile<'_>> {
    let crates = workspace.crates();
    let mut found = Vec::new();
    for (krate, naming) in crates
        .iter()
        .filter_map(|krate| Some((krate, naming(krate)?)))
    {
        for file in workspace.sources_of(krate) {
            let Some(within) = file.path.strip_prefix(&format!("{}/src/", krate.path)) else {
                continue;
            };
            if file.production().is_none() {
                continue;
            }
            let path = match &naming {
                Naming::FromSrc => within.to_owned(),
                Naming::Under(name) => format!("{name}/{within}"),
            };
            found.push(NamedFile { path, file });
        }
    }
    found
}
