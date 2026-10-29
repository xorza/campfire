//! The rule of a state part's name: its declaring module's name before its own.

use syn::{ImplItem, Item, Lit};

use super::{Naming, capability};
use crate::source_file::Gate;
use crate::workspace::Workspace;

/// The crate whose part impls the orphan rule puts in a lower crate: `values` implements `units`'
/// traits for its own types (02-engine-core.md, A plain type's state impls sit beside it).
const ORPHANS: [(&str, &str); 1] = [("values", "units")];

pub(crate) fn a_parts_name_takes_its_declaring_modules_prefix(workspace: &Workspace) {
    let mut wrong = Vec::new();
    for krate in workspace.crates() {
        let Some(Naming::Under(module)) = capability(krate) else {
            continue;
        };
        let declaring = ORPHANS
            .iter()
            .find(|&&(lower, _)| lower == module)
            .map_or(module.as_str(), |&(_, declaring)| declaring);
        for source in workspace.sources_of(krate) {
            if source.production().is_none() {
                continue;
            }
            let file = syn::parse_file(&source.text).expect("a workspace source parses");
            for name in part_names(&file.items) {
                if name.split_once('.').map(|(prefix, _)| prefix) != Some(declaring) {
                    wrong.push(format!("{}: {name}", source.path));
                }
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// The `NAME` of each `SimComponent` and `SimResource` impl of `items` and of their modules.
fn part_names(items: &[Item]) -> Vec<String> {
    let mut names = Vec::new();
    for item in items {
        match item {
            Item::Impl(item) => {
                let Some((path, _)) = &item.trait_ else {
                    continue;
                };
                let last = path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string());
                if !matches!(last.as_deref(), Some("SimComponent" | "SimResource")) {
                    continue;
                }
                for inner in &item.items {
                    if let ImplItem::Const(constant) = inner
                        && constant.ident == "NAME"
                        && let syn::Expr::Lit(literal) = &constant.expr
                        && let Lit::Str(text) = &literal.lit
                    {
                        names.push(text.value());
                    }
                }
            }
            // A gated module's parts are a test's or a bench's.
            Item::Mod(module) if !module.attrs.iter().any(|attr| Gate::of(attr).is_some()) => {
                if let Some((_, inner)) = &module.content {
                    names.extend(part_names(inner));
                }
            }
            _ => {}
        }
    }
    names
}

#[test]
fn a_part_name_is_read_of_its_impl_and_of_a_module_within() {
    let code = "impl SimComponent for A { const NAME: &'static str = \"units.a\"; }\n\
                impl Other for B { const NAME: &'static str = \"x.b\"; }\n\
                mod m { impl campfire_sim::SimResource for C { const NAME: &'static str = \"c\"; } }\n\
                #[cfg(not(test))] mod p { impl SimComponent for D { const NAME: &'static str = \"d\"; } }\n\
                #[cfg(test)] mod t { impl SimComponent for E { const NAME: &'static str = \"e\"; } }";
    let file = syn::parse_file(code).unwrap();
    // A module of a bare test gate is a test's; one of `not(test)` is production code.
    assert_eq!(part_names(&file.items), ["units.a", "c", "d"]);
}
