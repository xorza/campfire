use proc_macro2::Ident;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Fields, ImplItemFn, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, ItemType,
    Signature, TraitItemFn, Type,
};

use crate::workspace::Workspace;

/// Each name of `text` that breaks its suffix: a system set not named `*Systems`; a plugin not
/// named `*Plugin` and a plugin group not named `*Plugins`; a `*Manager`; a `*_hook` function whose
/// signature names no script hook, a `Hook*` type; and a `*Kind` that wraps a `TypeId`, which is a
/// `*TypeId` (02-engine-core.md, A system set is `*Systems`; 02-engine-core.md, A Bevy plugin is a
/// `*Plugin`, a group `*Plugins`; 02-engine-core.md, A hook is a script's).
fn misnamed(text: &str) -> Vec<String> {
    // A file that names none of the words a misnamed declaration holds declares none, and its
    // parse is skipped.
    let words = ["SystemSet", "Manager", "TypeId", "Plugin", "_hook"];
    if !words.iter().any(|word| text.contains(word)) {
        return Vec::new();
    }
    let file = syn::parse_file(text).expect("a workspace source parses");
    let mut names = Names::default();
    names.visit_file(&file);
    names.0
}

/// The names a walk of one file found misnamed.
#[derive(Debug, Default)]
struct Names(Vec<String>);

impl Names {
    /// Takes the type `name`, which `attributes` derive for: a system set's or any type's.
    fn take_type(&mut self, name: &str, attributes: &[Attribute]) {
        if derives(attributes, "SystemSet") && !name.ends_with("Systems") {
            self.0.push(format!("the system set {name}"));
        }
        if name.ends_with("Manager") {
            self.0.push(format!("the manager {name}"));
        }
    }

    /// Takes the function of `signature`.
    fn take_fn(&mut self, signature: &Signature) {
        let name = signature.ident.to_string();
        let mut hooks = HookTypes(false);
        hooks.visit_signature(signature);
        if name.ends_with("_hook") && !hooks.0 {
            self.0.push(format!("the function {name}"));
        }
    }
}

impl<'ast> Visit<'ast> for Names {
    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        let name = item.ident.to_string();
        self.take_type(&name, &item.attrs);
        if let Fields::Unnamed(fields) = &item.fields
            && name.ends_with("Kind")
            && fields.unnamed.len() == 1
            && let Type::Path(wrapped) = &fields.unnamed[0].ty
            && wrapped.path.is_ident("TypeId")
        {
            self.0.push(format!("the type id {name}"));
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
        let suffix = match implemented.as_deref() {
            Some("Plugin") => Some("Plugin"),
            Some("PluginGroup") => Some("Plugins"),
            _ => None,
        };
        if let (Some(suffix), Type::Path(path)) = (suffix, &*item.self_ty)
            && let Some(segment) = path.path.segments.last()
            && !segment.ident.to_string().ends_with(suffix)
        {
            self.0.push(format!("the plugin {}", segment.ident));
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
    let mut found = Vec::new();
    let mut files = 0;
    for file in workspace.sources() {
        files += 1;
        for name in misnamed(&file.text) {
            found.push(format!("{}: {name}", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // The walk reads the workspace, so a moved tree fails here, not silently.
    assert!(files > 1000, "{files}");

    // Each misnamed declaration is found, and its well-named twin passes.
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
    ];
    for (code, count) in samples {
        assert_eq!(misnamed(code).len(), count, "{code}");
    }
}

/// The suffixes of a package's data as a package writes it (02-engine-core.md, A `*Spec` is a
/// resolved section, a `*Setup` what a thing starts from).
const DATA_SUFFIXES: [&str; 4] = ["Data", "File", "Patch", "Manifest"];

/// The structs of `code`, out of its test modules, that read a package's data, by a name of `DATA_SUFFIXES`, and refuse no
/// field they do not hold: a struct that derives `Deserialize` with neither
/// `#[serde(deny_unknown_fields)]` nor `#[serde(transparent)]`, which takes its one field's.
fn open_data(code: &str) -> Vec<String> {
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
    let file = syn::parse_file(code).expect("a workspace source parses");
    let mut open = Open(Vec::new());
    open.visit_file(&file);
    open.0
}

pub(crate) fn a_package_data_struct_refuses_a_field_it_does_not_hold(workspace: &Workspace) {
    let found: Vec<String> = workspace
        .sources()
        .iter()
        .filter(|file| file.text.contains("Deserialize") && file.production().is_some())
        .flat_map(|file| {
            open_data(&file.text)
                .into_iter()
                .map(move |name| format!("{}: {name}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // A data struct that refuses an unknown field passes, and one that wraps its one field; one
    // that takes any field is found, and a struct of no data's name is not.
    let code = concat!(
        "#[derive(Deserialize)]\n#[serde(deny_unknown_fields, rename_all = \"snake_case\")]\nstruct AData { a: u8 }\n",
        "#[derive(Deserialize)]\n#[serde(transparent)]\nstruct BData(u8);\n",
        "#[derive(Debug, Deserialize)]\nstruct CFile { c: u8 }\n",
        "#[derive(Deserialize)]\nstruct Fields { d: u8 }\n",
        "#[cfg(test)]\nmod tests {\n    #[derive(Deserialize)]\n    struct File { e: u8 }\n}\n",
    );
    assert_eq!(open_data(code), ["CFile"]);
}
