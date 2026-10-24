use syn::visit::Visit;
use syn::{ItemExternCrate, ItemMacro, ItemUse, UseTree, Visibility};

use crate::manifests::COMMON_DERIVE;
use crate::workspace::Workspace;

/// A `pub use` of any visibility, or a `pub extern crate`, of a file: its line, and the first
/// segment of each path it names, with whether that path is that one name alone.
#[derive(Debug)]
struct PubUse {
    line: usize,
    roots: Vec<(String, bool)>,
}

/// The `pub use`s and `pub extern crate`s of `text`, at any depth of its modules, and the names
/// of the `macro_rules!` it defines.
fn pub_uses(text: &str) -> (Vec<PubUse>, Vec<String>) {
    #[derive(Default)]
    struct Uses {
        uses: Vec<PubUse>,
        macros: Vec<String>,
    }
    fn roots(tree: &UseTree, found: &mut Vec<(String, bool)>) {
        match tree {
            UseTree::Path(path) => found.push((path.ident.to_string(), false)),
            UseTree::Name(name) => found.push((name.ident.to_string(), true)),
            UseTree::Rename(rename) => found.push((rename.ident.to_string(), true)),
            UseTree::Group(group) => group.items.iter().for_each(|tree| roots(tree, found)),
            UseTree::Glob(_) => {}
        }
    }
    impl<'ast> Visit<'ast> for Uses {
        fn visit_item_use(&mut self, item: &'ast ItemUse) {
            if !matches!(item.vis, Visibility::Inherited) {
                let mut found = Vec::new();
                roots(&item.tree, &mut found);
                self.uses.push(PubUse {
                    line: item.use_token.span.start().line,
                    roots: found,
                });
            }
        }

        fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
            if !matches!(item.vis, Visibility::Inherited) {
                self.uses.push(PubUse {
                    line: item.extern_token.span.start().line,
                    roots: vec![(item.ident.to_string(), false)],
                });
            }
        }

        fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
            if item.mac.path.is_ident("macro_rules")
                && let Some(name) = &item.ident
            {
                self.macros.push(name.to_string());
            }
        }
    }
    // A file of no `pub use`, of any reach, holds none, and its parse is skipped.
    if !text.contains("pub use") && !text.contains(") use ") {
        return (Vec::new(), Vec::new());
    }
    let file = syn::parse_file(text).expect("a workspace source parses");
    let mut uses = Uses::default();
    uses.visit_file(&file);
    (uses.uses, uses.macros)
}

/// The names that start a path of the crate's own.
const OWN_ROOTS: [&str; 3] = ["crate", "self", "super"];

/// The lines of `text` that re-export another crate, whole or in part: a `pub use`, of any
/// visibility, of a path that starts with a name other than `crate`, `self` or `super`, and a
/// `pub extern crate`. A `use` of one name that a `macro_rules!` of `text` defines gives the macro
/// a path, as a macro defined in a module takes one, and names no crate.
fn foreign_re_exports(text: &str) -> Vec<usize> {
    let (uses, macros) = pub_uses(text);
    let mut lines: Vec<usize> = uses
        .iter()
        .filter(|used| {
            used.roots.iter().any(|(root, alone)| {
                !OWN_ROOTS.contains(&root.as_str()) && (!*alone || !macros.contains(root))
            })
        })
        .map(|used| used.line)
        .collect();
    lines.dedup();
    lines
}

/// Whether `line` of the file at `path` is the one re-export of another crate the rule allows:
/// `common`'s of the derives of its own traits, from the proc-macro crate that alone may hold
/// them, which no other crate names (02-engine-core.md, Structural rules).
fn derives_of_common(path: &str, line: &str) -> bool {
    let from = format!("pub use {}::", COMMON_DERIVE.replace('-', "_"));
    path == "crates/base/common/src/lib.rs" && line.trim_start().starts_with(&from)
}

pub(crate) fn no_crate_re_exports_another_crate(workspace: &Workspace) {
    let found: Vec<String> = workspace
        .sources()
        .iter()
        .flat_map(|file| {
            let lines: Vec<&str> = file.text.lines().collect();
            foreign_re_exports(&file.text)
                .into_iter()
                .filter(move |&line| !derives_of_common(&file.path, lines[line - 1]))
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // A whole crate, an item, a scoped one, an indented group and an extern crate are found; the
    // crate's own, a macro's own, a plain import and the path of a macro the file defines are not.
    let sample = "pub use rhai;\npub use crate::a::B;\npub(crate) use ::toml::de::Error as E;\nmod m {\n    pub use other::{A, B};\n}\npub use self::x;\nuse other::C;\npub use super::y;\npub extern crate rhai;\nmacro_rules! local {\n    () => { pub use $crate::Item; };\n}\npub(crate) use local;\npub(crate) use toml;";
    assert_eq!(foreign_re_exports(sample), [1, 3, 5, 10, 15]);
}

/// The lines of `text`, the source at `path`, that re-export an item of their own crate outside
/// its root: a `pub use` of any visibility of a path from `crate`, `self` or `super` in a file
/// other than a `lib.rs` or `main.rs`, as one canonical path holds each item and only the root's
/// `pub use`s publish it (02-engine-core.md, Structural rules).
fn inner_re_exports(path: &str, text: &str) -> Vec<usize> {
    if path.ends_with("/lib.rs") || path.ends_with("/main.rs") {
        return Vec::new();
    }
    let (uses, _) = pub_uses(text);
    uses.iter()
        .filter(|used| (used.roots.iter()).any(|(root, _)| OWN_ROOTS.contains(&root.as_str())))
        .map(|used| used.line)
        .collect()
}

pub(crate) fn no_module_re_exports_its_own_crates_item(workspace: &Workspace) {
    let found: Vec<String> = workspace
        .sources()
        .iter()
        .filter(|file| file.path != "crates/test/rules/src/re_exports.rs")
        .flat_map(|file| {
            inner_re_exports(&file.path, &file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // Each visibility of a re-export in a module is found, and a root's, a plain `use` and
    // another crate's path are not.
    let sample = "pub(crate) use crate::a::B;\npub use self::c::D;\n    pub(super) use super::E;\nuse crate::f::G;\npub use other::H;\n";
    assert_eq!(inner_re_exports("crates/x/src/m.rs", sample), [1, 2, 3]);
    assert!(inner_re_exports("crates/x/src/lib.rs", sample).is_empty());
}
