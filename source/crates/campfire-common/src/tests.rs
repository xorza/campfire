use std::fs;

#[test]
fn the_crate_depends_on_serde_and_derive_more_alone() {
    // Read by hand, as the crate has no TOML parser. Only the listed sections may appear, so
    // a dependency in any other form, such as `[dependencies.name]`, a target's section or
    // a dotted key before the first section, fails the test instead of passing it unseen.
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
    let manifest = manifest.unwrap();
    let mut section = "";
    let mut names = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            section = header.strip_suffix(']').unwrap();
            let known = ["package", "dependencies", "dev-dependencies", "lints"];
            assert!(known.contains(&section), "section [{section}]");
        } else {
            assert!(!section.is_empty(), "{line:?} before the first section");
            if section == "dependencies" {
                names.push(line.split(['.', ' ', '=']).next().unwrap());
            }
        }
    }
    assert_eq!(names, ["derive_more", "serde"]);
}

#[test]
fn the_design_marks_built_exactly_the_crates_of_the_workspace() {
    // Design 02's module table names each module in its first column and its status in the
    // second; a built module's crate is `campfire-<module>` in `crates/` or `checks/`.
    let source = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let design = fs::read_to_string(format!("{source}/../docs/design/02-engine-core.md")).unwrap();
    let table = design
        .lines()
        .skip_while(|line| *line != "| Module | Status | Does |")
        .skip(2)
        .take_while(|line| line.starts_with('|'));
    let mut built = Vec::new();
    for row in table {
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        match cells[2] {
            "built" => built.push(format!("campfire-{}", cells[1].trim_matches('`'))),
            "planned" => {}
            status => panic!("{status:?} is no status, in {row}"),
        }
    }
    built.sort_unstable();
    let mut crates = Vec::new();
    for folder in ["crates", "checks"] {
        for entry in fs::read_dir(format!("{source}/{folder}")).unwrap() {
            let entry = entry.unwrap();
            if entry.path().join("Cargo.toml").is_file() {
                crates.push(entry.file_name().into_string().unwrap());
            }
        }
    }
    crates.sort_unstable();
    assert_eq!(built, crates);
}

/// The functions only `store` calls (design 02, Storage): those that write, sync, rename
/// or remove a file or make a directory, and those that start a thread.
const STORAGE_RULES: [&str; 23] = [
    "std::fs::write",
    "std::fs::copy",
    "std::fs::rename",
    "std::fs::hard_link",
    "std::fs::remove_file",
    "std::fs::remove_dir",
    "std::fs::remove_dir_all",
    "std::fs::create_dir",
    "std::fs::create_dir_all",
    "std::fs::set_permissions",
    "std::fs::DirBuilder::new",
    "std::fs::File::create",
    "std::fs::File::create_new",
    "std::fs::File::options",
    "std::fs::File::set_len",
    "std::fs::File::sync_all",
    "std::fs::File::sync_data",
    "std::fs::File::set_permissions",
    "std::fs::OpenOptions::new",
    "std::thread::spawn",
    "std::thread::scope",
    "std::thread::Builder::spawn",
    "std::thread::Builder::spawn_scoped",
];

/// The paths of `rules` that the `disallowed-methods` list of `clippy`, a `clippy.toml`, does not
/// name.
fn unlisted<'a>(clippy: &str, rules: &[&'a str]) -> Vec<&'a str> {
    let list = clippy
        .split_once("disallowed-methods = [")
        .and_then(|(_, rest)| rest.split_once("\n]"))
        .map_or("", |(list, _)| list);
    let named = |path: &&str| list.contains(&format!("path = \"{path}\""));
    rules.iter().copied().filter(|path| !named(path)).collect()
}

/// The lint the storage rules' list feeds, and the groups that hold it, named in parts so that
/// this file's own samples hold no attribute of them.
const LINT: &str = concat!("clippy::", "disallowed_methods");
const GROUPS: [&str; 2] = [concat!("clippy::", "style"), concat!("clippy::", "all")];

/// The lines of `text`, the source at `path` under `source/`, whose attribute exempts code from
/// the lint other than as the rules let it: only `expect` of the lint itself, never `allow`, a
/// `cfg_attr` or a group that holds it, so a stale one fails and none hides; for a whole crate
/// only in `store`, which owns the rules, and in an integration test's root; for a module only
/// on one of `#[cfg(test)]`; else on an item or a statement.
fn misplaced_exemptions(path: &str, text: &str) -> Vec<usize> {
    let line_of = |at: usize| text[..at].matches('\n').count() + 1;
    let mut misplaced = Vec::new();
    for name in [LINT].iter().chain(&GROUPS) {
        for (at, _) in text.match_indices(name) {
            if text[at + name.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            // Only an attribute counts: `#[` or `#![` before the name, with no `]` between, so
            // the attribute is still open; else it is a mention, in a comment or a string.
            let Some(start) = text[..at].rfind('#') else {
                continue;
            };
            let opening = &text[start..at];
            let inner = opening.starts_with("#![");
            let Some(kind) = opening
                .strip_prefix(if inner { "#![" } else { "#[" })
                .filter(|_| !opening.contains(']'))
                .and_then(|head| head.split_once('('))
                .map(|(kind, _)| kind.trim())
            else {
                continue;
            };
            if ["warn", "deny", "forbid"].contains(&kind) {
                continue;
            }
            if *name != LINT || kind != "expect" {
                misplaced.push(line_of(at));
                continue;
            }
            if inner {
                let integration_root = matches!(
                    path.split('/').collect::<Vec<_>>()[..],
                    [_, _, "tests", "mod.rs"]
                );
                if path != "crates/campfire-store/src/lib.rs" && !integration_root {
                    misplaced.push(line_of(at));
                }
                continue;
            }
            // The attributes after it, then the item they belong to.
            let mut rest = &text[at..];
            rest = &rest[rest.find(']').expect("an attribute closes") + 1..];
            let mut test_only = false;
            while let Some(next) = rest.trim_start().strip_prefix("#[") {
                test_only |= next.starts_with("cfg(test)]");
                rest = &next[next.find(']').expect("an attribute closes") + 1..];
            }
            let item = rest.trim_start();
            let module = ["mod ", "pub mod ", "pub(crate) mod "]
                .iter()
                .any(|kind| item.starts_with(kind));
            if module && !test_only {
                misplaced.push(line_of(at));
            }
        }
    }
    misplaced.sort_unstable();
    misplaced
}

#[test]
fn only_store_writes_a_file_or_starts_a_thread() {
    let source = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let clippy = fs::read_to_string(format!("{source}/clippy.toml")).unwrap();
    assert_eq!(unlisted(&clippy, &STORAGE_RULES), Vec::<&str>::new());
    let mut folders: Vec<String> = ["crates", "checks"].map(String::from).to_vec();
    let mut misplaced = Vec::new();
    let mut read = Vec::new();
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(format!("{source}/{folder}")).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let path = format!("{folder}/{name}");
            if entry.file_type().unwrap().is_dir() {
                if name != "target" {
                    folders.push(path);
                }
            } else if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rs")
            {
                let text = fs::read_to_string(entry.path()).unwrap();
                read.push(path.clone());
                misplaced.extend(
                    misplaced_exemptions(&path, &text)
                        .into_iter()
                        .map(|line| format!("{path}:{line}")),
                );
            }
        }
    }
    assert_eq!(misplaced, Vec::<String>::new());
    assert!(
        read.iter()
            .any(|path| path == "crates/campfire-store/src/lib.rs")
    );
}

#[test]
fn the_storage_rules_checks_fail_on_what_breaks_them() {
    // A list that misses a path, and each misplaced exemption.
    let one = "disallowed-methods = [\n    { path = \"std::fs::write\", reason = \"r\" },\n]\n";
    assert_eq!(
        unlisted(one, &["std::fs::write", "std::fs::copy"]),
        ["std::fs::copy"]
    );
    let expect = format!("#[expect({LINT}, reason = \"r\")]\n");
    let inner = format!("#!{}", &expect[1..]);
    let cases = [
        (
            "crates/x/src/a.rs",
            format!("{expect}fn f() {{}}\n"),
            vec![],
        ),
        (
            "crates/x/src/a.rs",
            format!("#[expect(\n    {LINT},\n    reason = \"r\"\n)]\nlet x = 1;\n"),
            vec![],
        ),
        (
            "crates/x/src/a.rs",
            format!("#[allow({LINT})]\nfn f() {{}}\n"),
            vec![1],
        ),
        ("crates/x/src/lib.rs", format!("//! x\n{inner}"), vec![2]),
        ("crates/campfire-store/src/lib.rs", inner.clone(), vec![]),
        ("crates/x/tests/mod.rs", inner.clone(), vec![]),
        ("crates/x/src/a.rs", format!("{expect}mod io;\n"), vec![1]),
        (
            "crates/x/src/a.rs",
            format!("{expect}pub(crate) mod io {{}}\n"),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!("{expect}#[cfg(test)]\nmod tests;\n"),
            vec![],
        ),
        (
            "crates/x/src/a.rs",
            format!("/// Mentions `{LINT}`.\nfn f() {{}}\n"),
            vec![],
        ),
        // Turned off under a condition, or with another lint on a module: both found.
        (
            "crates/x/src/a.rs",
            format!("#[cfg_attr(test, allow({LINT}))]\nfn f() {{}}\n"),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!("#[expect(clippy::other, {LINT}, reason = \"r\")]\nmod io;\n"),
            vec![1],
        ),
        // A group that holds the lint, but a longer name that starts with one's.
        (
            "crates/x/src/a.rs",
            format!(
                "#![allow({})]\n#[expect({})]\nfn f() {{}}\n",
                GROUPS[1], GROUPS[0]
            ),
            vec![1, 2],
        ),
        (
            "crates/x/src/a.rs",
            format!("#[expect({}ow_attributes)]\nfn f() {{}}\n", GROUPS[1]),
            vec![],
        ),
        // A level that does not turn the lint off.
        (
            "crates/x/src/a.rs",
            format!("#[deny({LINT})]\nmod io;\n"),
            vec![],
        ),
        // A unit test's directory is no integration test's root.
        ("crates/x/src/a/tests/mod.rs", inner, vec![1]),
    ];
    for (path, text, lines) in cases {
        assert_eq!(misplaced_exemptions(path, &text), lines, "{text}");
    }
}
