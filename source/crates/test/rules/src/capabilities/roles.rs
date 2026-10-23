use super::{NamedCode, capability_sources};
use crate::allowlist::Allowlist;

/// Each role a module's type may hold, by the trait it implements or, for a script API, by the
/// `*Api` type that registers members: the suffix of the type's name, and of its file's.
const ROLES: [(&str, &str, &str); 4] = [
    ("impl Effect for ", "Effect", "effect"),
    ("impl ViewColumn for ", "Column", "column"),
    ("impl CallExtension for ", "Call", "call"),
    ("struct ", "Api", "api"),
];

/// The types that hold a role but are named for what they are: the core's script API, which spans
/// `units`, `scripts` and `players`.
const ROLE_EXCEPTIONS: [&str; 1] = ["CoreApi"];

/// The types of `code`, the file `file` of the module `module`, that hold a role and are not
/// named `<Module><Role>` in `<module>_<role>.rs`, as `(file, type)`; a `mod.rs` is named by its
/// directory, and a crate's `lib.rs` by its module.
fn misnamed_roles(
    module: &str,
    file: &str,
    code: &str,
    exceptions: &Allowlist<&str>,
) -> Vec<(String, String)> {
    let pascal: String = module
        .split('_')
        .map(|word| word[..1].to_ascii_uppercase() + &word[1..])
        .collect();
    let mut misnamed = Vec::new();
    for (opening, suffix, file_suffix) in ROLES {
        if suffix == "Api" && !code.contains("fn register(api: &mut ApiBuilder") {
            continue;
        }
        for (at, _) in code.match_indices(opening) {
            if at > 0 && !code[..at].ends_with(|c: char| c.is_whitespace() || c == ')') {
                continue;
            }
            let rest = &code[at + opening.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..end];
            if suffix == "Api" && !name.ends_with("Api") {
                continue;
            }
            let named =
                name == format!("{pascal}{suffix}") && file == format!("{module}_{file_suffix}");
            if !named && !exceptions.allows(|exception| *exception == name) {
                misnamed.push((file.to_owned(), name.to_owned()));
            }
        }
    }
    misnamed
}

#[test]
fn each_role_of_a_module_takes_the_modules_name() {
    let exceptions = Allowlist::new(&ROLE_EXCEPTIONS);
    let mut misnamed = Vec::new();
    let mut held = [0; ROLES.len()];
    for NamedCode { path, code } in capability_sources() {
        let parts: Vec<&str> = path.split('/').collect();
        let [module, .., last] = parts[..] else {
            continue;
        };
        let file = match last.strip_suffix(".rs").expect("a source ends in .rs") {
            "mod" => parts[parts.len() - 2],
            "lib" => module,
            stem => stem,
        };
        misnamed.extend(misnamed_roles(module, file, &code, &exceptions));
        for (held, (opening, ..)) in held.iter_mut().zip(ROLES) {
            *held += code.matches(opening).count();
        }
    }
    assert_eq!(misnamed, Vec::<(String, String)>::new());
    assert_eq!(exceptions.unused(), Vec::<&&str>::new());
    // The walk reads each role, so a role it no longer finds fails here, not silently.
    assert!(held.iter().all(|&count| count > 0), "{held:?}");

    // Each role is found where it is misnamed, a script API only where it registers members,
    // and the exceptions pass.
    let api = |name: &str| {
        format!("struct {name};\nimpl {name} {{ fn register(api: &mut ApiBuilder<'_>) {{}} }}")
    };
    let samples = [
        (
            "vision",
            "vision_effect",
            "impl Effect for VisionEffect {".to_owned(),
            vec![],
        ),
        (
            "vision",
            "reveal_effect",
            "impl Effect for RevealEffect {".to_owned(),
            vec![("reveal_effect", "RevealEffect")],
        ),
        (
            "vision",
            "vision_column",
            "impl ViewColumn for SightColumn {".to_owned(),
            vec![("vision_column", "SightColumn")],
        ),
        (
            "units",
            "units_call",
            "impl CallExtension for UnitsCall {".to_owned(),
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
            "pub struct ScriptApi {".to_owned(),
            vec![],
        ),
        (
            "orders",
            "unit_order",
            "impl Effect for UnitOrder {".to_owned(),
            vec![("unit_order", "UnitOrder")],
        ),
        (
            "orders",
            "orders_effect",
            "impl Effect for OrdersEffect {".to_owned(),
            vec![],
        ),
        (
            "capability_set",
            "capability_set",
            "impl Effect for CapabilitySetEffect {".to_owned(),
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
            misnamed_roles(module, file, &code, &exceptions),
            expected,
            "{code}"
        );
    }
}

/// Each role whose types take a suffix, wherever they are: the trait a type implements to hold
/// it, and the suffix of its name. A `*Book` is exactly a book, a `*Data` a table of package data.
const SUFFIX_ROLES: [(&str, &str); 2] = [("Book", "Book"), ("DataSection", "Data")];

/// The types that hold a suffix role but are named for what they are: the generic book of a value
/// for each id.
const SUFFIX_EXCEPTIONS: [&str; 1] = ["ById"];

/// The types `code` implements `role` for, by name: each `impl … <role> for <type>`.
fn implementers<'c>(code: &'c str, role: &str) -> Vec<&'c str> {
    let opening = format!(" {role} for ");
    let mut found = Vec::new();
    for (at, _) in code.match_indices(&opening) {
        let line = code[..at].rsplit('\n').next().unwrap_or_default();
        if !line.trim_start().starts_with("impl") {
            continue;
        }
        let rest = &code[at + opening.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        found.push(&rest[..end]);
    }
    found
}

/// The types `code` declares whose name ends in `suffix`.
fn suffixed<'c>(code: &'c str, suffix: &str) -> Vec<&'c str> {
    let mut found = Vec::new();
    for (at, _) in code.match_indices("struct ") {
        let rest = &code[at + "struct ".len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if rest[..end].ends_with(suffix) {
            found.push(&rest[..end]);
        }
    }
    found
}

#[test]
fn a_book_and_a_data_table_take_their_suffix() {
    let mut holders: [Vec<String>; SUFFIX_ROLES.len()] = Default::default();
    let mut books = Vec::new();
    for NamedCode { code, .. } in capability_sources() {
        for (holders, (role, _)) in holders.iter_mut().zip(SUFFIX_ROLES) {
            holders.extend(implementers(&code, role).into_iter().map(str::to_owned));
        }
        books.extend(suffixed(&code, "Book").into_iter().map(str::to_owned));
    }
    let suffix_exceptions = Allowlist::new(&SUFFIX_EXCEPTIONS);
    let mut misnamed = Vec::new();
    for (holders, (_, suffix)) in holders.iter().zip(SUFFIX_ROLES) {
        assert!(
            !holders.is_empty(),
            "the walk finds the holders of each role"
        );
        let wrong = holders.iter().filter(|name| {
            !name.ends_with(suffix) && !suffix_exceptions.allows(|exception| exception == name)
        });
        misnamed.extend(wrong.cloned());
    }
    assert_eq!(suffix_exceptions.unused(), Vec::<&&str>::new());
    assert_eq!(
        misnamed,
        Vec::<String>::new(),
        "a holder of a role misnamed"
    );
    // Every `*Book` is a book.
    let no_books: Vec<_> = books
        .iter()
        .filter(|name| !holders[0].contains(name))
        .collect();
    assert_eq!(no_books, Vec::<&String>::new(), "a `*Book` that is no book");

    // A generic impl and a plain one are found, and a mention in a comment is not.
    let code = "impl<T: PerId<UnitType>> Book for ById<UnitType, T> {}\nimpl Book for ActionBook {}\n// a Book for all";
    assert_eq!(implementers(code, "Book"), ["ById", "ActionBook"]);
    let code = "pub(crate) struct ModeBooks;\nstruct StatBook(u8);";
    assert_eq!(suffixed(code, "Book"), ["StatBook"]);
}
