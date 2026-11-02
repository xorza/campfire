//! The traits whose every method a module or a match must write: a default would let a new one
//! forget it (AGENTS.md, What can be forgotten is a type or a trait method with no default).

use syn::visit::{self, Visit};
use syn::{ItemTrait, TraitItem};

use crate::allowlist::Allowlist;
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

/// The methods of `code`'s traits that `listed` takes that have a default, as `Trait::method`.
fn defaults(code: &str, listed: impl Fn(&str) -> bool) -> Vec<String> {
    struct Defaults<F> {
        listed: F,
        found: Vec<String>,
    }
    impl<'ast, F: Fn(&str) -> bool> Visit<'ast> for Defaults<F> {
        fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
            let name = item.ident.to_string();
            if (self.listed)(&name) {
                for member in &item.items {
                    if let TraitItem::Fn(method) = member
                        && method.default.is_some()
                    {
                        self.found.push(format!("{name}::{}", method.sig.ident));
                    }
                }
            }
            visit::visit_item_trait(self, item);
        }
    }
    let file = syn::parse_file(code).expect("a workspace source parses");
    let mut found = Defaults {
        listed,
        found: Vec::new(),
    };
    found.visit_file(&file);
    found.found
}

pub(crate) fn a_declaration_trait_has_no_default_method(workspace: &Workspace) {
    let listed = Allowlist::new(&NO_DEFAULTS);
    let mut found = Vec::new();
    for file in workspace.sources() {
        if !NO_DEFAULTS.iter().any(|(path, _)| *path == file.path) {
            continue;
        }
        let code = file
            .production()
            .expect("a trait's file is production code");
        let named =
            |name: &str| listed.allows(|(path, listed)| *path == file.path && *listed == name);
        found.extend(
            defaults(code, named)
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
    assert_eq!(defaults(code, |name| name == "Module"), ["Module::declare"]);
}
