use std::collections::BTreeMap;

use proc_macro2::Ident;
use syn::visit::{self, Visit};
use syn::{Attribute, FnArg, ImplItem, ItemFn, ItemImpl, ItemMod, Signature, Type, Visibility};

use crate::source_file::{Gate, SourceFile};
use crate::workspace::Workspace;

/// A function whose first parameter is a world: its line, its name with its owner's, as
/// `Owner::name`, or alone for a free function, and whether code past its module may call it.
#[derive(Debug, PartialEq, Eq)]
struct WorldFirst {
    line: usize,
    name: String,
    visible: bool,
}

/// Each function of `text` whose first parameter is a world, a reference to a `World`: a free
/// function at any depth, or a method of an inherent `impl`, not of a trait's; none of an item
/// that a gate holds, which only tests and their helpers build.
fn world_first(text: &str) -> Vec<WorldFirst> {
    struct Functions(Vec<WorldFirst>);
    impl Functions {
        fn take(&mut self, owner: Option<&Ident>, visibility: &Visibility, signature: &Signature) {
            let Some(FnArg::Typed(first)) = signature.inputs.first() else {
                return;
            };
            let Type::Reference(reference) = &*first.ty else {
                return;
            };
            if !matches!(&*reference.elem, Type::Path(referent) if referent.path.is_ident("World"))
            {
                return;
            }
            let name = signature.ident.to_string();
            let visible = match visibility {
                Visibility::Public(_) => true,
                Visibility::Restricted(restricted) => !restricted.path.is_ident("super"),
                Visibility::Inherited => false,
            };
            self.0.push(WorldFirst {
                line: signature.fn_token.span.start().line,
                name: owner.map_or_else(|| name.clone(), |owner| format!("{owner}::{name}")),
                visible,
            });
        }
    }
    fn gated(attributes: &[Attribute]) -> bool {
        attributes
            .iter()
            .any(|attribute| Gate::of(attribute).is_some())
    }
    impl<'ast> Visit<'ast> for Functions {
        fn visit_item_fn(&mut self, item: &'ast ItemFn) {
            if gated(&item.attrs) {
                return;
            }
            self.take(None, &item.vis, &item.sig);
            visit::visit_item_fn(self, item);
        }

        fn visit_item_mod(&mut self, item: &'ast ItemMod) {
            if !gated(&item.attrs) {
                visit::visit_item_mod(self, item);
            }
        }

        fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
            if gated(&item.attrs) {
                return;
            }
            let owner = match &*item.self_ty {
                Type::Path(path) if item.trait_.is_none() => path.path.segments.last(),
                _ => None,
            };
            for inner in &item.items {
                if let ImplItem::Fn(function) = inner
                    && !gated(&function.attrs)
                {
                    if let Some(owner) = owner {
                        self.take(Some(&owner.ident), &function.vis, &function.sig);
                    }
                    visit::visit_block(self, &function.block);
                }
            }
        }
    }
    // A file that names no world takes none first, and its parse is skipped.
    if !text.contains("World") {
        return Vec::new();
    }
    let file = syn::parse_file(text).expect("a workspace source parses");
    let mut functions = Functions(Vec::new());
    functions.visit_file(&file);
    functions.0
}

/// Whether `text` names the function `name`, as `Owner::name` or alone, as a value: Bevy
/// registers a system, an observer, a hook or a run condition by value, and a table holds a
/// function so.
fn named_as_value(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].trim_end();
        let after = text[at + name.len()..].trim_start();
        let whole = !text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':')
            && !text[at + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
        let called = after.starts_with(['(', ':', '<']);
        let listed = before.ends_with(['(', ',', '[']) || name.contains("::");
        let line = &text[text[..at].rfind('\n').map_or(0, |end| end + 1)..at];
        let comment = line.trim_start().starts_with("//");
        whole && !called && listed && !comment
    })
}

pub(crate) fn a_free_function_that_takes_a_world_first_is_a_system(workspace: &Workspace) {
    // The world rule (02-engine-core.md, Structural rules): what acts on the match's content is a
    // function of its module's type, and a value made from the world a constructor of its own. A
    // free function past its module that takes the world first is a system or a function a table
    // holds, named as a value.
    let sources = workspace.sources();
    let production: Vec<(&SourceFile, String)> = sources
        .iter()
        .filter_map(|file| Some((file, file.production()?)))
        .collect();
    let mut found = Vec::new();
    for (file, _) in &production {
        let free = world_first(&file.text)
            .into_iter()
            .filter(|f| f.visible && !f.name.contains("::"));
        for function in free {
            let valued = production
                .iter()
                .any(|(_, text)| named_as_value(text, &function.name));
            if !valued {
                found.push(format!("{}:{} {}", file.path, function.line, function.name));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

/// Each trait `text` declares that the world rule refuses, by its line and name: a test trait
/// on the world, `<Module>WorldTest`; a crate's internals trait on it,
/// `<Module>WorldCrateInternals`; and a module's trait on `Match`, `<Module>Match` or
/// `<Module>MatchInternals`, as a match a module builds comes from the module's type.
fn refused_traits(text: &str) -> Vec<(usize, &str)> {
    const REFUSED: [&str; 4] = [
        "WorldTest",
        "WorldCrateInternals",
        "Match",
        "MatchInternals",
    ];
    let mut found = Vec::new();
    for (line, code) in text.lines().enumerate() {
        let mut code = code.trim_start();
        if let Some(rest) = code.strip_prefix("pub") {
            let rest = rest.strip_prefix('(').map_or(rest, |inner| {
                inner.split_once(')').map_or(inner, |(_, after)| after)
            });
            code = rest.trim_start();
        }
        let Some(rest) = code.strip_prefix("trait ") else {
            continue;
        };
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let refused = REFUSED
            .iter()
            .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix));
        if refused {
            found.push((line + 1, name));
        }
    }
    found
}

pub(crate) fn no_trait_is_a_test_trait_on_the_world_or_a_modules_trait_on_a_match(
    workspace: &Workspace,
) {
    // The world rule (02-engine-core.md, Structural rules): a world trait holds the whole match and
    // the fast access to a module's systems, for other crates, its own crate, or the tests and
    // benches of any crate; a test's verbs belong to its fixture or its capability's type, and a
    // match a module builds to the module's type. Test code counts, as these traits lived in it;
    // this file holds a source of each form for the check's own test.
    let mut found = Vec::new();
    let own = "crates/test/rules/src/world.rs";
    for file in workspace.sources().iter().filter(|file| file.path != own) {
        for (line, name) in refused_traits(&file.text) {
            found.push(format!("{}:{line} {name}", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_world_rule_finds_each_trait_it_refuses() {
    // Each refused form, at any visibility; the world traits it keeps, a trait named `Match`
    // alone and a type of such a name are none.
    let text = "pub trait UnitsWorldTest {}
\
        pub(crate) trait ActionsWorldCrateInternals {}
\
        \x20   trait SessionMatch: Sized {}
\
        pub(super) trait HarnessMatchInternals {}
\
        pub trait SimWorld {}
pub(crate) trait PlayersWorldCrate {}
\
        pub trait SimWorldInternals {}
trait Match {}
pub struct ScriptedMatch;
";
    assert_eq!(
        refused_traits(text),
        [
            (1, "UnitsWorldTest"),
            (2, "ActionsWorldCrateInternals"),
            (3, "SessionMatch"),
            (4, "HarnessMatchInternals"),
        ]
    );
}

/// Each use of a `<Module>WorldCrate` trait outside the crate that declares it, among `files`, each
/// a source by its crate's name, its path and its text: by the path, the line and the trait. Such a
/// trait is its own crate's alone (02-engine-core.md, Structural rules): another crate takes the
/// module's `<Module>World`, and a test or a bench its `<Module>WorldInternals`.
fn foreign_crate_traits<'a>(
    files: &[(&'a str, &'a str, &'a str)],
) -> Vec<(&'a str, usize, &'a str)> {
    let identifiers = |line: &'a str| {
        line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty())
    };
    let crate_trait = |name: &str| name.len() > "WorldCrate".len() && name.ends_with("WorldCrate");
    let mut declared = BTreeMap::new();
    for &(owner, _, text) in files {
        for line in text.lines() {
            let mut words = identifiers(line);
            while let Some(word) = words.next() {
                if word == "trait"
                    && let Some(name) = words.next()
                    && crate_trait(name)
                {
                    declared.insert(name, owner);
                }
            }
        }
    }
    let mut found = Vec::new();
    for &(user, path, text) in files {
        for (line, code) in text.lines().enumerate() {
            for word in identifiers(code) {
                if declared.get(word).is_some_and(|&owner| owner != user) {
                    found.push((path, line + 1, word));
                }
            }
        }
    }
    found
}

pub(crate) fn no_crate_names_another_crates_own_world_trait(workspace: &Workspace) {
    let own = "crates/test/rules/src/world.rs";
    let files: Vec<&SourceFile> = workspace
        .sources()
        .iter()
        .filter(|file| file.path != own)
        .collect();
    let files: Vec<(&str, &str, &str)> = files
        .iter()
        .filter_map(|file| {
            let holder = workspace.holder(&file.path)?;
            Some((holder.name.as_str(), file.path.as_str(), file.text.as_str()))
        })
        .collect();
    let found: Vec<String> = foreign_crate_traits(&files)
        .into_iter()
        .map(|(path, line, name)| format!("{path}:{line} {name}"))
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_crate_trait_rule_finds_each_use_outside_its_crate() {
    // `a` declares `AWorldCrate` and uses it; `b` imports it and names it in a bound; `c` names
    // `AWorldCrateInternals` and `AWorld`, other traits, and `XWorldCrate`, which no crate
    // declares.
    let files = [
        (
            "a",
            "a.rs",
            "pub(crate) trait AWorldCrate {}
impl AWorldCrate for World {}
",
        ),
        (
            "b",
            "b.rs",
            "use a::{AWorld, AWorldCrate};
fn f<W: AWorldCrate>() {}
",
        ),
        (
            "c",
            "c.rs",
            "use a::AWorldCrateInternals;
use a::AWorld;
use x::XWorldCrate;
",
        ),
    ];
    assert_eq!(
        foreign_crate_traits(&files),
        [("b.rs", 1, "AWorldCrate"), ("b.rs", 2, "AWorldCrate")]
    );
}

#[test]
fn the_world_rule_finds_each_function_that_takes_a_world_first() {
    // A free function and methods, of each form of a reference to a world, their visibility
    // read from their modifiers; a method of a trait's impl, one that takes the world later,
    // one that takes a world by value and one a gate holds are none.
    let text = "pub fn a(world: &World) {}\n\
        impl<'w> Owner<'w> {\n    pub(crate) const fn b<T>(\n        w: &'w mut World,\n    ) {}\n\
        \x20   pub(super) fn c(world: &mut World, x: u8) {}\n    fn d(x: u8, world: &World) {}\n}\n\
        impl Trait for Owner {\n    fn e(world: &World) {}\n}\n\
        fn f(world: World) {}\nfn g(world: &'a World) {}\n#[cfg(test)]\nfn h(world: &World) {}\n";
    let found = world_first(text);
    let expected = [
        (1, "a", true),
        (3, "Owner::b", true),
        (6, "Owner::c", false),
        (13, "g", false),
    ];
    let expected: Vec<WorldFirst> = expected
        .into_iter()
        .map(|(line, name, visible)| WorldFirst {
            line,
            name: name.to_owned(),
            visible,
        })
        .collect();
    assert_eq!(found, expected);
    // A name passed or listed as a value counts; a call, a path through it, a longer name and a
    // comment's mention do not.
    assert!(named_as_value(".add_systems(Owner::run)", "Owner::run"));
    assert!(named_as_value("[a, run, b]", "run"));
    assert!(!named_as_value("Owner::run(world)", "Owner::run"));
    assert!(!named_as_value("Owner::run::<T>(world)", "Owner::run"));
    assert!(!named_as_value(
        ".add_systems(Owner::running)",
        "Owner::run"
    ));
    assert!(!named_as_value("let x = run;", "run"));
    assert!(!named_as_value("/// As `Owner::run` does.", "Owner::run"));
}
