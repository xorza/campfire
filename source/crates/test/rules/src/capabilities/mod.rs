//! The rules of the capability crates' code: where their sets are named, their roles' and books'
//! names, their lookups by name, and the words of a genre they leave to data.

use crate::workspace::{Crate, Workspace};

pub(crate) mod genre_words;
pub(crate) mod lookups;
pub(crate) mod roles;
pub(crate) mod sets;

/// The capability crates' group under `source/`.
const GROUP: &str = "crates/capabilities/";

/// The top crate of the group, whose files go by their paths from its `src`.
const TOP: &str = "crates/capabilities/capabilities";

/// A source file's production code, by its path as these rules name it.
#[derive(Debug)]
struct NamedCode {
    path: String,
    code: String,
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
        (krate.path.starts_with(GROUP)).then(|| Naming::Under(krate.dir_name.replace('-', "_")))
    }
}

/// The production code of each capability crate's sources, of `workspace`.
fn capability_sources(workspace: &Workspace) -> Vec<NamedCode> {
    named_code(workspace, capability)
}

/// The production code of the sources of each crate of `workspace` that `naming` names, by the
/// path it gives.
fn named_code(workspace: &Workspace, naming: impl Fn(&Crate) -> Option<Naming>) -> Vec<NamedCode> {
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
            let Some(code) = file.production() else {
                continue;
            };
            let path = match &naming {
                Naming::FromSrc => within.to_owned(),
                Naming::Under(name) => format!("{name}/{within}"),
            };
            found.push(NamedCode {
                path,
                code: code.to_owned(),
            });
        }
    }
    found
}
