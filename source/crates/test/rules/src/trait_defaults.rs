//! The traits whose every method a module or a match must write: a default would let a new one
//! forget it (AGENTS.md, What can be forgotten is a type or a trait method with no default).

use syn::visit::{self, Visit};
use syn::{ItemTrait, TraitItem};

use crate::allowlist::Allowlist;
use crate::parsed_code::ParsedCode;
use crate::workspace::Workspace;

/// The traits no method of has a default, each by the file that declares it.
const NO_DEFAULTS: [(&str, &str); 8] = [
    (
        "crates/capabilities/core/declarations/src/module.rs",
        "Module",
    ),
    (
        "crates/capabilities/core/declarations/src/module.rs",
        "ModuleVisitor",
    ),
    (
        "crates/capabilities/core/declarations/src/declarations.rs",
        "Declarations",
    ),
    (
        "crates/capabilities/core/declarations/src/module_systems.rs",
        "ModuleSystems",
    ),
    (
        "crates/capabilities/core/declarations/src/kind_visitor.rs",
        "KindVisitor",
    ),
    ("crates/capabilities/core/states/src/lib.rs", "StateTypes"),
    ("crates/capabilities/core/states/src/lib.rs", "SendingWays"),
    ("crates/sim/sim/src/own_resources.rs", "OwnResources"),
];

/// A trait a file declares, by its name, with the methods of it that have a default.
#[derive(Debug)]
pub(crate) struct TraitDefaults {
    name: String,
    defaults: Vec<String>,
}

/// The traits of `production`, the production code of the file at `path`, each with its methods
/// that have a default; none of a file that declares no trait of `NO_DEFAULTS`.
pub(crate) fn defaults(path: &str, production: &ParsedCode<'_>) -> Vec<TraitDefaults> {
    struct Defaults(Vec<TraitDefaults>);
    impl<'ast> Visit<'ast> for Defaults {
        fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
            let defaults = (item.items.iter())
                .filter_map(|member| match member {
                    TraitItem::Fn(method) if method.default.is_some() => {
                        Some(method.sig.ident.to_string())
                    }
                    _ => None,
                })
                .collect();
            self.0.push(TraitDefaults {
                name: item.ident.to_string(),
                defaults,
            });
            visit::visit_item_trait(self, item);
        }
    }
    if !NO_DEFAULTS.iter().any(|(listed, _)| *listed == path) {
        return Vec::new();
    }
    let mut found = Defaults(Vec::new());
    found.visit_file(production.file());
    found.0
}

/// The methods of `traits`, the traits of the file at `path`, that have a default, of each trait
/// `listed` takes there, as `Trait::method`.
fn listed_defaults(
    path: &str,
    traits: &[TraitDefaults],
    listed: &Allowlist<(&str, &str)>,
) -> Vec<String> {
    traits
        .iter()
        .filter(|item| listed.allows(|&(file, name)| file == path && name == item.name))
        .flat_map(|item| (item.defaults.iter()).map(|method| format!("{}::{method}", item.name)))
        .collect()
}

pub(crate) fn a_declaration_trait_has_no_default_method(workspace: &Workspace) {
    let listed = Allowlist::new(&NO_DEFAULTS);
    let mut found = Vec::new();
    for file in workspace.sources() {
        let methods = listed_defaults(&file.path, &file.findings().trait_defaults, &listed);
        found.extend(
            methods
                .into_iter()
                .map(|method| format!("{}: {method}", file.path)),
        );
    }
    assert_eq!(found, Vec::<String>::new());
    // Each listed trait is found where the list says, so a moved one fails here, not silently.
    assert_eq!(listed.unused(), Vec::<&(&str, &str)>::new());
}

#[test]
fn a_default_method_of_a_listed_trait_is_found() {
    // A listed trait's default is found, its required methods and another trait's default are
    // not.
    let code = "trait Module { fn install(app: &mut App); fn declare(d: &mut D) {} }\n\
                trait Other { fn helper(&self) {} }\n";
    let path = NO_DEFAULTS[0].0;
    let traits = defaults(path, &ParsedCode::new(path, code));
    let listed = Allowlist::new(&NO_DEFAULTS);
    assert_eq!(listed_defaults(path, &traits, &listed), ["Module::declare"]);
    assert!(defaults("crates/x/src/a.rs", &ParsedCode::new("a.rs", code)).is_empty());
}
