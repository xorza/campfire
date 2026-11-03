use syn::visit::{self, Visit};
use syn::{ImplItemFn, ItemImpl, ItemStruct, Type};

use crate::allowlist::Allowlist;
use crate::capabilities::{NamedFile, capability_sources, in_capability};
use crate::parsed_code::ParsedCode;
use crate::workspace::Workspace;

/// Each role a module's type may hold, by the trait it implements or, for a script API, by
/// `API_ROLE`: the trait, the suffix of the type's name, and of its file's.
const ROLES: [(&str, &str, &str); 3] = [
    ("Effect", "Effect", "effect"),
    ("ViewColumn", "Column", "column"),
    ("CallExtension", "Call", "call"),
];

/// The role of a script API: a `*Api` struct of a file whose `register` function takes an
/// `ApiBuilder`, the suffix of its name, and of its file's.
const API_ROLE: (&str, &str) = ("Api", "api");

/// The types that hold a role but are named for what they are: the core's script API, which spans
/// `units`, `scripts` and `players`.
const ROLE_EXCEPTIONS: [&str; 1] = ["CoreApi"];

/// A type that holds a role: its name, the suffix of its role, and its file's.
#[derive(Debug)]
pub(crate) struct RoleHolder {
    name: String,
    suffix: &'static str,
    file_suffix: &'static str,
}

/// The types of one file that hold a role: the implementers of the traits of `ROLES`, and the
/// `*Api` structs of a file that registers members.
#[derive(Debug, Default)]
pub(crate) struct RoleHolders {
    holders: Vec<RoleHolder>,
    api_structs: Vec<String>,
    registers: bool,
}

impl RoleHolders {
    /// The role holders of `production`, the production code of the file at `path`, a
    /// capability crate's; none of another file.
    pub(crate) fn of(path: &str, production: &ParsedCode<'_>) -> Vec<RoleHolder> {
        if !in_capability(path) {
            return Vec::new();
        }
        let mut found = Self::default();
        found.visit_file(production.file());
        let (suffix, file_suffix) = API_ROLE;
        if found.registers {
            found
                .holders
                .extend((found.api_structs.into_iter()).map(|name| RoleHolder {
                    name,
                    suffix,
                    file_suffix,
                }));
        }
        found.holders
    }
}

impl<'ast> Visit<'ast> for RoleHolders {
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let implemented = (item.trait_.as_ref())
            .and_then(|(path, _)| path.segments.last())
            .map(|segment| segment.ident.to_string());
        let role = ROLES
            .iter()
            .find(|(role, ..)| implemented.as_deref() == Some(role));
        if let (Some(&(_, suffix, file_suffix)), Type::Path(path)) = (role, &*item.self_ty)
            && let Some(segment) = path.path.segments.last()
        {
            self.holders.push(RoleHolder {
                name: segment.ident.to_string(),
                suffix,
                file_suffix,
            });
        }
        visit::visit_item_impl(self, item);
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        let name = item.ident.to_string();
        if name.ends_with(API_ROLE.0) {
            self.api_structs.push(name);
        }
        visit::visit_item_struct(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        let takes_builder = item.sig.inputs.iter().any(|input| {
            let mut builder = NamesBuilder(false);
            builder.visit_fn_arg(input);
            builder.0
        });
        self.registers |= item.sig.ident == "register" && takes_builder;
        visit::visit_impl_item_fn(self, item);
    }
}

/// Whether a type names an `ApiBuilder`.
#[derive(Debug)]
struct NamesBuilder(bool);

impl<'ast> Visit<'ast> for NamesBuilder {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        self.0 |= segment.ident == "ApiBuilder";
        visit::visit_path_segment(self, segment);
    }
}

/// The types of `holders`, the role holders of the file `file` of the module `module`, that are
/// not named `<Module><Role>` in `<module>_<role>.rs`, as `(file, type)`; a `mod.rs` is named by
/// its directory, and a crate's `lib.rs` by its module.
fn misnamed_roles(
    module: &str,
    file: &str,
    holders: &[RoleHolder],
    exceptions: &Allowlist<&str>,
) -> Vec<(String, String)> {
    let pascal: String = module
        .split('_')
        .map(|word| word[..1].to_ascii_uppercase() + &word[1..])
        .collect();
    let mut misnamed = Vec::new();
    for RoleHolder {
        name,
        suffix,
        file_suffix,
    } in holders
    {
        let named =
            *name == format!("{pascal}{suffix}") && file == format!("{module}_{file_suffix}");
        if !named && !exceptions.allows(|exception| exception == name) {
            misnamed.push((file.to_owned(), name.clone()));
        }
    }
    misnamed
}

pub(crate) fn each_role_of_a_module_takes_the_modules_name(workspace: &Workspace) {
    let exceptions = Allowlist::new(&ROLE_EXCEPTIONS);
    let mut misnamed = Vec::new();
    let mut held = [0; ROLES.len() + 1];
    for NamedFile { path, file } in capability_sources(workspace) {
        let parts: Vec<&str> = path.split('/').collect();
        let [module, .., last] = parts[..] else {
            continue;
        };
        let name = match last.strip_suffix(".rs").expect("a source ends in .rs") {
            "mod" => parts[parts.len() - 2],
            "lib" => module,
            stem => stem,
        };
        let holders = &file.findings().role_holders;
        misnamed.extend(misnamed_roles(module, name, holders, &exceptions));
        let suffixes = ROLES.iter().map(|&(_, suffix, _)| suffix);
        for (held, suffix) in held.iter_mut().zip(suffixes.chain([API_ROLE.0])) {
            *held += holders
                .iter()
                .filter(|holder| holder.suffix == suffix)
                .count();
        }
    }
    assert_eq!(misnamed, Vec::<(String, String)>::new());
    assert_eq!(exceptions.unused(), Vec::<&&str>::new());
    // The walk reads each role, so a role it no longer finds fails here, not silently.
    assert!(held.iter().all(|&count| count > 0), "{held:?}");
}

#[test]
fn each_misnamed_role_is_found() {
    // Each role is found where it is misnamed, a script API only where it registers members,
    // and the exceptions pass.
    let api = |name: &str| {
        format!("struct {name};\nimpl {name} {{ fn register(api: &mut ApiBuilder<'_>) {{}} }}")
    };
    let samples = [
        (
            "vision",
            "vision_effect",
            "impl Effect for VisionEffect {}".to_owned(),
            vec![],
        ),
        (
            "vision",
            "reveal_effect",
            "impl Effect for RevealEffect {}".to_owned(),
            vec![("reveal_effect", "RevealEffect")],
        ),
        (
            "vision",
            "vision_column",
            "impl ViewColumn for SightColumn {}".to_owned(),
            vec![("vision_column", "SightColumn")],
        ),
        (
            "units",
            "units_call",
            "impl CallExtension for UnitsCall {}".to_owned(),
            vec![],
        ),
        ("units", "units_api", api("UnitsApi"), vec![]),
        (
            "units",
            "units_api",
            api("PositionApi"),
            vec![("units_api", "PositionApi")],
        ),
        (
            "units",
            "position_api",
            api("UnitsApi"),
            vec![("position_api", "UnitsApi")],
        ),
        ("scripts", "core_api", api("CoreApi"), vec![]),
        (
            "scripts",
            "script_api",
            "pub struct ScriptApi {}".to_owned(),
            vec![],
        ),
        (
            "orders",
            "unit_order",
            "impl Effect for UnitOrder {}".to_owned(),
            vec![("unit_order", "UnitOrder")],
        ),
        (
            "orders",
            "orders_effect",
            "impl Effect for OrdersEffect {}".to_owned(),
            vec![],
        ),
        (
            "capability_set",
            "capability_set",
            "impl Effect for CapabilitySetEffect {}".to_owned(),
            vec![("capability_set", "CapabilitySetEffect")],
        ),
    ];
    for (module, file, code, expected) in samples {
        let expected: Vec<(String, String)> = expected
            .into_iter()
            .map(|(file, name): (&str, &str)| (file.to_owned(), name.to_owned()))
            .collect();
        let exceptions = Allowlist::new(&ROLE_EXCEPTIONS);
        assert_eq!(
            misnamed_roles(
                module,
                file,
                &RoleHolders::of(
                    "crates/capabilities/x/src/a.rs",
                    &ParsedCode::new("a.rs", &code)
                ),
                &exceptions
            ),
            expected,
            "{code}"
        );
    }
}
