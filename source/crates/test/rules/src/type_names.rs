use std::collections::{BTreeMap, BTreeSet};

use campfire_threads::Parallel;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Ident, ItemEnum, ItemMod, ItemStruct, ItemTrait, ItemType, ItemUnion, Visibility,
};

use crate::source_file::Gate;
use crate::workspace::Workspace;

/// The names of the types, traits and aliases `text` declares visible past their module, but in a
/// module only its crate's tests build: a private item is its module's word alone.
fn declared(text: &str) -> BTreeSet<String> {
    let file = syn::parse_file(text).expect("a workspace source parses");
    let mut names = Declared::default();
    names.visit_file(&file);
    names.0
}

/// The names a walk of one file found declared.
#[derive(Debug, Default)]
struct Declared(BTreeSet<String>);

/// Whether `attributes` gate their item to the crate's own tests.
fn tests_only(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| Gate::of(attribute) == Some(Gate::Test))
}

impl Declared {
    fn take(&mut self, visibility: &Visibility, name: &Ident) {
        if !matches!(visibility, Visibility::Inherited) {
            self.0.insert(name.to_string());
        }
    }
}

impl<'ast> Visit<'ast> for Declared {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if !tests_only(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        self.take(&item.vis, &item.ident);
        visit::visit_item_struct(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        self.take(&item.vis, &item.ident);
        visit::visit_item_enum(self, item);
    }

    fn visit_item_union(&mut self, item: &'ast ItemUnion) {
        self.take(&item.vis, &item.ident);
        visit::visit_item_union(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        self.take(&item.vis, &item.ident);
        visit::visit_item_trait(self, item);
    }

    fn visit_item_type(&mut self, item: &'ast ItemType) {
        self.take(&item.vis, &item.ident);
        visit::visit_item_type(self, item);
    }
}

/// The shapes that each crate declares one of, with one meaning in each (02-engine-core.md, A
/// shared shape keeps its name in each crate).
const ONE_MEANING: [&str; 9] = [
    "Args",
    "CheckError",
    "CheckFailure",
    "CommandLine",
    "ConsoleArg",
    "Golden",
    "Iter",
    "RowReads",
    "Sealed",
];

/// Whether the file at `path`, under `source/`, is a test's or a bench's alone.
fn tests_file(path: &str) -> bool {
    path.contains("/tests/") || path.contains("/benches/") || path.ends_with("/tests.rs")
}

/// Each name that two crates or more of `files` declare, each a crate's name and a source's text,
/// with the crates that declare it.
fn shared<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<String, BTreeSet<String>> {
    let files: Vec<(&str, &str)> = files.into_iter().collect();
    // Each file's parse on every core, as it reads every file of the workspace.
    let declared = Parallel::map(files.len(), |at| declared(files[at].1));
    let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (&(holder, _), names) in files.iter().zip(declared) {
        for name in names {
            by_name.entry(name).or_default().insert(holder.to_owned());
        }
    }
    by_name.retain(|_, holders| holders.len() > 1);
    by_name
}

/// Each name `shared` holds that `one_meaning` does not list, and each entry of `one_meaning` that
/// no two crates declare.
fn broken(shared: &BTreeMap<String, BTreeSet<String>>, one_meaning: &[&str]) -> Vec<String> {
    let unlisted = shared
        .iter()
        .filter(|(name, _)| !one_meaning.contains(&name.as_str()))
        .map(|(name, holders)| format!("{name}: declared by {holders:?}"));
    let stale = one_meaning
        .iter()
        .filter(|name| !shared.contains_key(**name))
        .map(|name| format!("{name}: listed, and declared by no two crates"));
    unlisted.chain(stale).collect()
}

/// A name visible past its module that two crates declare means one thing, a shape `ONE_MEANING`
/// lists, or the narrower use takes a qualified name (02-engine-core.md, A type name that both
/// sides declare keeps one meaning).
pub(crate) fn a_type_name_keeps_one_meaning(workspace: &Workspace) {
    let sources = workspace.sources();
    let files = sources
        .iter()
        .filter(|file| !tests_file(&file.path))
        .map(|file| {
            let holder = workspace
                .holder(&file.path)
                .expect("each source is a crate's");
            (holder.name.as_str(), file.text.as_str())
        });
    assert_eq!(broken(&shared(files), &ONE_MEANING), Vec::<String>::new());

    // Samples: a name two crates declare past their modules, one only a module sees, one only a
    // crate's tests build, and one of a shape the list holds; and a listed shape no two declare.
    let a = "pub struct Clock;\nstruct Entry;\n#[cfg(test)]\nmod tests { pub struct Probe; }\n\
             pub(crate) struct Iter;\n";
    let b = "pub(crate) enum Clock { Tick }\nstruct Entry;\n#[cfg(test)]\nmod tests { pub struct Probe; }\n\
             pub struct Iter;\n";
    let found = shared([("a", a), ("b", b)]);
    assert_eq!(found.keys().collect::<Vec<_>>(), ["Clock", "Iter"]);
    assert_eq!(
        broken(&found, &["Iter", "Sealed"]),
        [
            "Clock: declared by {\"a\", \"b\"}",
            "Sealed: listed, and declared by no two crates",
        ]
    );
}

/// Each feature of a network stack crate among `crates`, each a crate's name and its features'
/// names, that takes the name of a crate of Campfire's.
fn taken_features(crates: &[(&str, Vec<&str>)]) -> Vec<String> {
    let campfire: BTreeSet<&str> = crates
        .iter()
        .map(|(name, _)| name.trim_start_matches("campfire-"))
        .filter(|name| !name.starts_with("net-"))
        .collect();
    crates
        .iter()
        .filter(|(name, _)| name.starts_with("campfire-net-"))
        .flat_map(|(name, features)| {
            (features.iter())
                .filter(|feature| campfire.contains(*feature))
                .map(move |feature| format!("{name}: {feature}"))
        })
        .collect()
}

/// No feature of the network stack's crates takes the name of a crate of Campfire's, as a word
/// keeps Campfire's meaning (02-engine-core.md, The stack's crates are `net-<name>`).
pub(crate) fn a_stack_feature_names_no_campfire_crate(workspace: &Workspace) {
    let crates = workspace.crates();
    let named: Vec<(&str, Vec<&str>)> = crates
        .iter()
        .map(|krate| {
            (
                krate.name.as_str(),
                krate.manifest.feature_names().collect(),
            )
        })
        .collect();
    assert_eq!(taken_features(&named), Vec::<String>::new());
    // A sample: the stack's feature named after the sim crate, and Campfire's own `net` crate's,
    // whose features are Campfire's words.
    let sample = [
        ("campfire-sim", vec![]),
        ("campfire-net", vec!["sim"]),
        ("campfire-net-stack", vec!["sim", "udp"]),
    ];
    assert_eq!(taken_features(&sample), ["campfire-net-stack: sim"]);
}
