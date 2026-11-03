use std::collections::BTreeSet;

use proc_macro2::Ident;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Fields, ImplItemFn, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, ItemType,
    Signature, TraitItemFn, Type,
};

use crate::allowlist::Allowlist;
use crate::parsed_code::ParsedCode;
use crate::source_file::SourceFile;
use crate::workspace::Workspace;

/// Each trait whose implementers take a suffix: the trait, the suffix, and what the rule calls a
/// holder. A `*Book` is exactly a book, and a `*Data` a table of package data.
const SUFFIXED_TRAITS: [(&str, &str, &str); 4] = [
    ("Plugin", "Plugin", "plugin"),
    ("PluginGroup", "Plugins", "plugin group"),
    ("Book", "Book", "book"),
    ("DataSection", "Data", "data section"),
];

/// The implementers of a suffixed trait that are named for what they are: the generic book of a
/// value for each id.
const SUFFIX_EXCEPTIONS: [&str; 1] = ["ById"];

/// The names of `text` that break their suffix: a system set not named `*Systems`; an
/// implementer of a trait of `SUFFIXED_TRAITS` not named by its suffix, which an exception may
/// pass; a `*Manager`; a `*_hook` function whose signature names no script hook, a `Hook*` type;
/// and a `*Kind` that wraps a `TypeId`, which is a `*TypeId` (02-engine-core.md, A system set is
/// `*Systems`; 02-engine-core.md, A Bevy plugin is a `*Plugin`, a group `*Plugins`;
/// 02-engine-core.md, A hook is a script's). With them, the `*Book` types it declares and the
/// types it implements `Book` for, which the workspace's walk matches.
pub(crate) fn misnamed(text: &ParsedCode<'_>) -> SuffixNames {
    let mut names = SuffixNames::default();
    // A file that names none of the words a misnamed declaration holds declares none, and its
    // parse is skipped.
    let words = [
        "SystemSet",
        "Manager",
        "TypeId",
        "Plugin",
        "_hook",
        "Book",
        "DataSection",
    ];
    if words.iter().any(|word| text.code.contains(word)) {
        names.visit_file(text.file());
    }
    names
}

/// What a walk of one file found: the names misnamed, the implementers misnamed, each with what
/// the rule calls its holder, which an exception may pass, the `*Book` types declared and the
/// types `Book` is implemented for.
#[derive(Debug, Default)]
pub(crate) struct SuffixNames {
    misnamed: Vec<String>,
    implementers: Vec<(&'static str, String)>,
    books: Vec<String>,
    book_impls: Vec<String>,
}

impl SuffixNames {
    /// The names misnamed, the implementers that `exceptions` passes left out.
    fn broken(&self, exceptions: &Allowlist<&'static str>) -> Vec<String> {
        let implementers = (self.implementers.iter())
            .filter(|(_, name)| !exceptions.allows(|listed| listed == name))
            .map(|(holder, name)| format!("the {holder} {name}"));
        self.misnamed.iter().cloned().chain(implementers).collect()
    }

    /// Takes the type `name`, which `attributes` derive for: a system set's or any type's.
    fn take_type(&mut self, name: &str, attributes: &[Attribute]) {
        if derives(attributes, "SystemSet") && !name.ends_with("Systems") {
            self.misnamed.push(format!("the system set {name}"));
        }
        if name.ends_with("Manager") {
            self.misnamed.push(format!("the manager {name}"));
        }
    }

    /// Takes the function of `signature`.
    fn take_fn(&mut self, signature: &Signature) {
        let name = signature.ident.to_string();
        let mut hooks = HookTypes(false);
        hooks.visit_signature(signature);
        if name.ends_with("_hook") && !hooks.0 {
            self.misnamed.push(format!("the function {name}"));
        }
    }
}

impl<'ast> Visit<'ast> for SuffixNames {
    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        let name = item.ident.to_string();
        self.take_type(&name, &item.attrs);
        if name.ends_with("Book") {
            self.books.push(name.clone());
        }
        if let Fields::Unnamed(fields) = &item.fields
            && name.ends_with("Kind")
            && fields.unnamed.len() == 1
            && let Type::Path(wrapped) = &fields.unnamed[0].ty
            && wrapped.path.is_ident("TypeId")
        {
            self.misnamed.push(format!("the type id {name}"));
        }
        visit::visit_item_struct(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        self.take_type(&item.ident.to_string(), &item.attrs);
        visit::visit_item_enum(self, item);
    }

    fn visit_item_type(&mut self, item: &'ast ItemType) {
        self.take_type(&item.ident.to_string(), &item.attrs);
        visit::visit_item_type(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        self.take_type(&item.ident.to_string(), &item.attrs);
        visit::visit_item_trait(self, item);
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let implemented = item
            .trait_
            .as_ref()
            .and_then(|(path, _)| path.segments.last())
            .map(|segment| segment.ident.to_string());
        let held = SUFFIXED_TRAITS
            .iter()
            .find(|(implemented_trait, ..)| implemented.as_deref() == Some(implemented_trait));
        if let (Some(&(implemented_trait, suffix, holder)), Type::Path(path)) =
            (held, &*item.self_ty)
            && let Some(segment) = path.path.segments.last()
        {
            let name = segment.ident.to_string();
            if implemented_trait == "Book" {
                self.book_impls.push(name.clone());
            }
            if !name.ends_with(suffix) {
                self.implementers.push((holder, name));
            }
        }
        visit::visit_item_impl(self, item);
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.take_fn(&item.sig);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.take_fn(&item.sig);
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        self.take_fn(&item.sig);
        visit::visit_trait_item_fn(self, item);
    }
}

/// Whether a signature names a script hook's type, a `Hook*`.
#[derive(Debug)]
struct HookTypes(bool);

impl<'ast> Visit<'ast> for HookTypes {
    fn visit_ident(&mut self, ident: &'ast Ident) {
        self.0 |= ident.to_string().starts_with("Hook");
    }
}

/// Whether `attributes` derive `name`, by its name alone or a path to it.
fn derives(attributes: &[Attribute], name: &str) -> bool {
    attributes.iter().any(|attribute| {
        let mut found = false;
        if attribute.path().is_ident("derive") {
            attribute
                .parse_nested_meta(|meta| {
                    found |= meta
                        .path
                        .segments
                        .last()
                        .is_some_and(|last| last.ident == name);
                    Ok(())
                })
                .expect("a derive lists paths");
        }
        found
    })
}

pub(crate) fn a_declared_name_keeps_its_suffix(workspace: &Workspace) {
    let exceptions = Allowlist::new(&SUFFIX_EXCEPTIONS);
    let mut found = Vec::new();
    let mut books = Vec::new();
    let mut book_impls = BTreeSet::new();
    let mut files = 0;
    for file in workspace.sources() {
        files += 1;
        let names = &file.findings().suffix_names;
        for name in names.broken(&exceptions) {
            found.push(format!("{}: {name}", file.path));
        }
        // A capability's `*Book` is its specs by id, a book; the client's are its own.
        if file.path.starts_with("crates/capabilities/") {
            let declared = names.books.iter();
            books.extend(declared.map(|book| (file.path.clone(), book)));
        }
        book_impls.extend(&names.book_impls);
    }
    // Every capability's `*Book` is a book.
    for (path, book) in books {
        if !book_impls.contains(book) {
            found.push(format!("{path}: the type {book}, no book"));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert_eq!(exceptions.unused(), Vec::<&&str>::new());
    // The walk reads the workspace, so a moved tree fails here, not silently.
    assert!(files > 1000, "{files}");
    assert!(book_impls.len() > 20, "{book_impls:?}");
}

#[test]
fn each_misnamed_declaration_is_found() {
    // Each misnamed declaration is found, and its well-named twin passes; a generic book and the
    // exception pass, and a mention in a comment is no implementer.
    let samples = [
        ("#[derive(SystemSet, Debug)]\nstruct DrawSet;", 1),
        ("#[derive(Debug, SystemSet)]\nenum DrawSystems { Paint }", 0),
        ("#[derive(bevy::ecs::schedule::SystemSet)]\nstruct Draw;", 1),
        (
            "impl Plugin for Hud { fn build(&self, app: &mut App) {} }",
            1,
        ),
        (
            "impl Plugin for HudPlugin { fn build(&self, app: &mut App) {} }",
            0,
        ),
        (
            "impl PluginGroup for Client { fn build(self) -> B { b } }",
            1,
        ),
        (
            "impl PluginGroup for ClientPlugins { fn build(self) -> B { b } }",
            0,
        ),
        ("struct PriorityManager;", 1),
        ("type PriorityManager = Vec<u8>;", 1),
        ("trait MessageManager {}", 1),
        ("trait Hooks { fn on_add_hook(&self); }", 1),
        ("fn on_insert_hook(world: DeferredWorld) {}", 1),
        ("impl A { fn plan_hook<H: HookDecl>(&mut self) {} }", 0),
        ("fn record_hook(hook: HookStatus) {}", 0),
        ("struct ChannelKind(TypeId);", 1),
        ("struct ChannelTypeId(TypeId);", 0),
        ("struct SlotKind(u8);", 0),
        ("impl Book for Stats {}", 1),
        ("impl Book for StatBook {}\n// a Book for all", 0),
        ("impl<T: PerId<UnitType>> Book for ById<UnitType, T> {}", 0),
        ("impl DataSection for Combat {}", 1),
        ("impl campfire_script::DataSection for CombatData {}", 0),
    ];
    let misnamed = |code: &str| misnamed(&ParsedCode::new("a.rs", code));
    for (code, count) in samples {
        let exceptions = Allowlist::new(&SUFFIX_EXCEPTIONS);
        assert_eq!(misnamed(code).broken(&exceptions).len(), count, "{code}");
    }
    // The `*Book` types a file declares and the types it implements `Book` for, a struct of
    // another suffix not among them.
    let names =
        misnamed("pub(crate) struct ModeBooks;\nstruct StatBook(u8);\nimpl Book for TagBook {}");
    assert_eq!(names.books, ["StatBook"]);
    assert_eq!(names.book_impls, ["TagBook"]);
}

/// The suffixes of a package's data as a package writes it (02-engine-core.md, A `*Spec` is a
/// resolved section, a `*Setup` what a thing starts from).
const DATA_SUFFIXES: [&str; 4] = ["Data", "File", "Patch", "Manifest"];

/// The structs of `text`, the source `file`'s, out of its test modules, that read a package's
/// data, by a name of `DATA_SUFFIXES`, and refuse no field they do not hold: a struct that derives
/// `Deserialize` with neither `#[serde(deny_unknown_fields)]` nor `#[serde(transparent)]`, which
/// takes its one field's; none of a file of no production code.
pub(crate) fn open_data(file: &SourceFile, text: &ParsedCode<'_>) -> Vec<String> {
    struct Open(Vec<String>);
    impl<'ast> Visit<'ast> for Open {
        fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
            let name = item.ident.to_string();
            let data = DATA_SUFFIXES.iter().any(|suffix| name.ends_with(suffix));
            let serde = |word: &str| {
                item.attrs.iter().any(|attribute| {
                    let mut found = false;
                    if attribute.path().is_ident("serde") {
                        attribute
                            .parse_nested_meta(|meta| {
                                found |= meta.path.is_ident(word);
                                if meta.input.peek(syn::Token![=]) {
                                    meta.value()?.parse::<syn::Expr>()?;
                                }
                                Ok(())
                            })
                            .expect("a serde attribute lists its options");
                    }
                    found
                })
            };
            if data
                && derives(&item.attrs, "Deserialize")
                && !serde("deny_unknown_fields")
                && !serde("transparent")
            {
                self.0.push(name);
            }
            visit::visit_item_struct(self, item);
        }

        /// A test module's structs read no package's data.
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            let test = item.attrs.iter().any(|attribute| {
                attribute.path().is_ident("cfg")
                    && attribute
                        .parse_args::<Ident>()
                        .is_ok_and(|gate| gate == "test")
            });
            if !test {
                visit::visit_item_mod(self, item);
            }
        }
    }
    if !text.code.contains("Deserialize") || file.production().is_none() {
        return Vec::new();
    }
    let mut open = Open(Vec::new());
    open.visit_file(text.file());
    open.0
}

pub(crate) fn a_package_data_struct_refuses_a_field_it_does_not_hold(workspace: &Workspace) {
    let found: Vec<String> = workspace
        .sources()
        .iter()
        .flat_map(|file| {
            (file.findings().open_data.iter()).map(move |name| format!("{}: {name}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn an_open_data_struct_is_found() {
    // A data struct that refuses an unknown field passes, and one that wraps its one field; one
    // that takes any field is found, and a struct of no data's name is not.
    let code = concat!(
        "#[derive(Deserialize)]\n#[serde(deny_unknown_fields, rename_all = \"snake_case\")]\nstruct AData { a: u8 }\n",
        "#[derive(Deserialize)]\n#[serde(transparent)]\nstruct BData(u8);\n",
        "#[derive(Debug, Deserialize)]\nstruct CFile { c: u8 }\n",
        "#[derive(Deserialize)]\nstruct Fields { d: u8 }\n",
        "#[cfg(test)]\nmod tests {\n    #[derive(Deserialize)]\n    struct File { e: u8 }\n}\n",
    );
    assert_eq!(
        open_data(
            &SourceFile::new("crates/x/src/a.rs", code),
            &ParsedCode::new("a.rs", code)
        ),
        ["CFile"]
    );
}
