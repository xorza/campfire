use std::collections::BTreeSet;

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{Ident, ImplItemFn, ItemImpl, ItemUse, Macro, ReturnType, Type};

use crate::capabilities::{NamedFile, capability_sources, in_capability};
use crate::parsed_code::ParsedCode;
use crate::workspace::Workspace;

/// What a file names of the sets: the sets it implements `ModuleSystems` for, the identifiers
/// it names outside its imports and its sets' places, a macro's among them, and whether it holds
/// a place, where a module names the steps of the modules it builds on.
#[derive(Debug, Default)]
pub(crate) struct SetNames {
    sets: Vec<String>,
    words: BTreeSet<String>,
    places: bool,
}

impl SetNames {
    /// What `production`, the production code of the file at `path`, a capability crate's, names
    /// of the sets; nothing of another file.
    pub(crate) fn of(path: &str, production: &ParsedCode<'_>) -> Self {
        let mut found = Self::default();
        if in_capability(path) {
            found.visit_file(production.file());
        }
        found
    }

    /// Takes each identifier of `tokens`, a macro's arguments, which no item holds.
    fn take_tokens(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Ident(ident) => {
                    self.words.insert(ident.to_string());
                }
                TokenTree::Group(group) => self.take_tokens(group.stream()),
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for SetNames {
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let implemented = (item.trait_.as_ref()).and_then(|(path, _)| path.segments.last());
        if implemented.is_some_and(|segment| segment.ident == "ModuleSystems")
            && let Type::Path(path) = &*item.self_ty
            && let Some(segment) = path.path.segments.last()
        {
            self.sets.push(segment.ident.to_string());
        }
        visit::visit_item_impl(self, item);
    }

    /// An import names a set for its place's sake.
    fn visit_item_use(&mut self, _: &'ast ItemUse) {}

    /// A set's place names the steps of the modules it builds on.
    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        let place = item.sig.ident == "place"
            && matches!(&item.sig.output, ReturnType::Type(_, output)
                if matches!(&**output, Type::Path(path) if path.path.is_ident("SetPlace")));
        if place {
            self.places = true;
        } else {
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_macro(&mut self, item: &'ast Macro) {
        self.take_tokens(item.tokens.clone());
        visit::visit_macro(self, item);
    }

    fn visit_ident(&mut self, ident: &'ast Ident) {
        self.words.insert(ident.to_string());
    }
}

pub(crate) fn a_module_names_another_modules_sets_only_where_its_sets_run(workspace: &Workspace) {
    // A set orders against the steps of the modules it builds on, which its place names; a module
    // puts its systems in its own sets alone, so no system orders against another module's but
    // through its set's place (02-engine-core.md, Sets order against the steps of the modules
    // they build on).
    let module_of = |path: &str| path.split('/').next().unwrap_or_default().to_owned();
    let files: Vec<(String, &SetNames)> = capability_sources(workspace)
        .into_iter()
        .map(|NamedFile { path, file }| (path, &file.findings().set_names))
        .collect();
    let sets: Vec<(String, &String)> = files
        .iter()
        .flat_map(|(path, names)| names.sets.iter().map(|set| (module_of(path), set)))
        .collect();
    assert!(
        sets.iter()
            .any(|(module, set)| module == "actions" && *set == "ActionsSystems")
    );
    let mut named = Vec::new();
    let mut placed = Vec::new();
    for (path, names) in &files {
        let module = module_of(path);
        if names.places {
            placed.push(module.clone());
        }
        for word in &names.words {
            // `NoSystems`, of a module that adds no system, is no module's.
            let other = sets
                .iter()
                .find(|(home, set)| *set == word && *home != module && home != "declarations");
            if let Some((home, set)) = other {
                named.push(format!("{path} names {home}'s {set}"));
            }
        }
    }
    assert_eq!(named, Vec::<String>::new());
    // The capabilities do place their sets at the steps of others: the scan sees them.
    assert!(placed.contains(&"combat".to_owned()), "{placed:?}");
}

#[test]
fn a_set_named_outside_its_place_and_imports_is_found() {
    // A set's place and an import name no set the file's words hold; a statement, a macro's
    // arguments and a type do.
    let names = SetNames::of(
        "crates/capabilities/x/src/a.rs",
        &ParsedCode::new(
            "a.rs",
            "use crate::{A, B};\nimpl ModuleSystems for CombatSystems {\n    fn place(self) -> SetPlace \
         {\n        SetPlace::at(C)\n    }\n}\nfn f(_: D) { g(E); assert!(F); }\n",
        ),
    );
    assert_eq!(names.sets, ["CombatSystems"]);
    assert!(names.places);
    for word in ["D", "E", "F", "CombatSystems"] {
        assert!(names.words.contains(word), "{word}");
    }
    for word in ["A", "B", "C"] {
        assert!(!names.words.contains(word), "{word}");
    }
}
