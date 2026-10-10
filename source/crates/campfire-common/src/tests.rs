use std::path::Path;

use campfire_store::{DirEntries, DirEntry, EntryKind, InputFile, SourceFile, SourceFiles};

/// The most bytes a test reads of a file of the workspace's own: far past any it holds.
const FILE_LEN: usize = 1 << 20;

/// The text of the workspace's file at `path`.
fn read(path: &str) -> String {
    InputFile::read_text(Path::new(path), FILE_LEN).unwrap()
}

#[test]
fn the_crate_depends_on_its_values_crates_and_the_codecs_alone() {
    // Read by hand, as the crate has no TOML parser. Only the listed sections may appear, so
    // a dependency in any other form, such as `[dependencies.name]`, a target's section or
    // a dotted key before the first section, fails the test instead of passing it unseen.
    let manifest = read(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
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
    assert_eq!(
        names,
        [
            "arrayvec",
            "derive_more",
            "postcard",
            "serde",
            "serde_json",
            "thiserror",
            "toml"
        ]
    );
}

#[test]
fn the_design_marks_built_exactly_the_crates_of_the_workspace() {
    // Design 02's module table names each module in its first column and its status in the
    // second; a built module's crate is `campfire-<module>` in `crates/` or `checks/`.
    let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let design = read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/design/02-engine-core.md"
    ));
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
        for entry in DirEntries::read(&source.join(folder)).unwrap() {
            let dir = source.join(folder).join(&entry.name);
            let manifest = |inner: &DirEntry| inner.name == "Cargo.toml";
            if entry.kind == EntryKind::Dir && DirEntries::read(&dir).unwrap().iter().any(manifest)
            {
                crates.push(entry.name.into_string().unwrap());
            }
        }
    }
    crates.sort_unstable();
    assert_eq!(built, crates);
}

/// The functions only `store` calls (design 02, Storage): those that read, list, check, write,
/// rename or remove a file or make a directory, `Path`'s probes, and those that start a thread.
const STORAGE_METHODS: [&str; 34] = [
    "std::fs::read",
    "std::fs::read_to_string",
    "std::fs::read_dir",
    "std::fs::read_link",
    "std::fs::metadata",
    "std::fs::symlink_metadata",
    "std::fs::canonicalize",
    "std::fs::exists",
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
    "std::path::Path::exists",
    "std::path::Path::try_exists",
    "std::path::Path::is_file",
    "std::path::Path::is_dir",
    "std::path::Path::is_symlink",
    "std::path::Path::metadata",
    "std::path::Path::symlink_metadata",
    "std::path::Path::read_dir",
    "std::path::Path::read_link",
    "std::path::Path::canonicalize",
    "tempfile::tempdir",
    "tempfile::tempfile",
    "std::thread::spawn",
    "std::thread::scope",
    "std::thread::Builder::spawn",
    "std::thread::Builder::spawn_scoped",
];

/// The types only `store` holds, each banned whole so every method of it is.
const STORAGE_TYPES: [&str; 8] = [
    "std::fs::File",
    "std::fs::OpenOptions",
    "std::fs::DirBuilder",
    "std::fs::ReadDir",
    "std::fs::DirEntry",
    "std::fs::Metadata",
    "tempfile::TempDir",
    "tempfile::NamedTempFile",
];

/// The paths of `rules` that the list `list`, `disallowed-methods` or `disallowed-types`, of
/// `clippy`, a `clippy.toml`, does not name.
fn unlisted<'a>(clippy: &str, list: &str, rules: &[&'a str]) -> Vec<&'a str> {
    let list = clippy
        .split_once(&format!("{list} = ["))
        .and_then(|(_, rest)| rest.split_once("\n]"))
        .map_or("", |(list, _)| list);
    let named = |path: &&str| list.contains(&format!("path = \"{path}\""));
    rules.iter().copied().filter(|path| !named(path)).collect()
}

/// The lints the storage rules' lists feed, and the groups that hold them, named in parts so
/// that this file's own samples hold no attribute of them.
const LINTS: [&str; 2] = [
    concat!("clippy::", "disallowed_methods"),
    concat!("clippy::", "disallowed_types"),
];
const GROUPS: [&str; 2] = [concat!("clippy::", "style"), concat!("clippy::", "all")];

/// The crate root whose `expect` of both lints lets `store` touch files and start threads.
const STORE_ROOT: &str = "crates/campfire-store/src/lib.rs";

/// The module root whose `expect` of `disallowed_methods` lets `common`'s `codec` call the
/// formats.
const CODEC_ROOT: &str = "crates/campfire-common/src/codec/mod.rs";

/// The one file outside `store` that may `expect` `disallowed_methods`, on a statement: the sim
/// schedule's build settings, which the same lint bans for another rule than storage's.
const SCHEDULE_SETTINGS: &str = "crates/campfire-sim/src/sim_update/mod.rs";

/// The lines of `text`, the source at `path` under `source/`, whose attribute exempts code from
/// either lint other than as the rules let it: only `expect` of a lint itself, never `allow`, a
/// `cfg_attr` or a group that holds it, so a stale one fails and none hides; of both for the
/// whole of `store`, which owns the rules, of `disallowed_methods` for the whole of `codec`, which
/// owns the formats, and in `SCHEDULE_SETTINGS`; nowhere else.
fn misplaced_exemptions(path: &str, text: &str) -> Vec<usize> {
    let line_of = |at: usize| text[..at].matches('\n').count() + 1;
    let mut misplaced = Vec::new();
    for name in LINTS.iter().chain(&GROUPS) {
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
            let allowed = LINTS.contains(name)
                && kind == "expect"
                && if inner {
                    path == STORE_ROOT || (*name == LINTS[0] && path == CODEC_ROOT)
                } else {
                    *name == LINTS[0] && path == SCHEDULE_SETTINGS
                };
            if !allowed {
                misplaced.push(line_of(at));
            }
        }
    }
    misplaced.sort_unstable();
    misplaced
}

/// Each Rust source file of the workspace's crates and checks, by its path under `source/`,
/// with its text.
fn sources() -> Vec<SourceFile> {
    let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut read = Vec::new();
    for folder in ["crates", "checks"] {
        for file in SourceFiles::under(&source.join(folder)) {
            read.push(SourceFile {
                path: format!("{folder}/{}", file.path),
                text: file.text,
            });
        }
    }
    read
}

#[test]
fn only_store_touches_a_file_or_starts_a_thread() {
    let clippy = read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../clippy.toml"));
    let methods = unlisted(&clippy, "disallowed-methods", &STORAGE_METHODS);
    assert_eq!(methods, Vec::<&str>::new());
    let types = unlisted(&clippy, "disallowed-types", &STORAGE_TYPES);
    assert_eq!(types, Vec::<&str>::new());
    let sources = sources();
    let misplaced: Vec<String> = sources
        .iter()
        .flat_map(|file| {
            misplaced_exemptions(&file.path, &file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(misplaced, Vec::<String>::new());
    assert!(sources.iter().any(|file| file.path == STORE_ROOT));
    assert!(sources.iter().any(|file| file.path == CODEC_ROOT));
}

/// The functions only `codec` calls (design 02, Serialization): each format's own encodes and
/// decodes, replicon's helpers over postcard, and Lightyear's registrations that take Lightyear's
/// own encoding.
const CODEC_METHODS: [&str; 24] = [
    "postcard::to_allocvec",
    "postcard::to_stdvec",
    "postcard::to_extend",
    "postcard::to_io",
    "postcard::to_slice",
    "postcard::serialize_with_flavor",
    "postcard::from_bytes",
    "postcard::take_from_bytes",
    "postcard::from_io",
    "toml::from_str",
    "toml::to_string",
    "toml::to_string_pretty",
    "serde_json::from_str",
    "serde_json::from_slice",
    "serde_json::from_reader",
    "serde_json::to_string",
    "serde_json::to_vec",
    "serde_json::to_writer",
    "bevy_replicon::postcard_utils::to_extend_mut",
    "bevy_replicon::postcard_utils::from_buf",
    "lightyear_messages::registry::AppMessageExt::register_message",
    "lightyear_replication::registry::replication::AppComponentExt::register_component",
    "lightyear_replication::registry::replication::ComponentRegistration::replicate",
    "lightyear_replication::registry::replication::ComponentRegistration::replicate_once",
];

/// The types only `codec` holds, each banned whole so every method of it is.
const CODEC_TYPES: [&str; 6] = [
    "postcard::Serializer",
    "postcard::Deserializer",
    "toml::Serializer",
    "toml::Deserializer",
    "serde_json::Serializer",
    "serde_json::Deserializer",
];

#[test]
fn only_the_codec_calls_a_format() {
    let clippy = read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../clippy.toml"));
    let methods = unlisted(&clippy, "disallowed-methods", &CODEC_METHODS);
    assert_eq!(methods, Vec::<&str>::new());
    let types = unlisted(&clippy, "disallowed-types", &CODEC_TYPES);
    assert_eq!(types, Vec::<&str>::new());
}

/// The one module whose code may differ by OS (design 02, Platform).
const PLATFORM: &str = "crates/campfire-store/src/platform/";

/// The words of a condition that name an OS, or the vendor or the toolchain environment that
/// stand for one, which only `PLATFORM` may test.
const OS_WORDS: [&str; 6] = [
    "unix",
    "windows",
    "target_os",
    "target_family",
    "target_vendor",
    "target_env",
];

/// The modules of std's `os` that one OS family alone has, which a grouped import names with no
/// `std` before them.
const OS_MODULES: [&str; 6] = ["unix", "windows", "fd", "linux", "macos", "wasi"];

/// The lines of `text` that name an OS: a `cfg`, `cfg!` or `cfg_attr` whose condition holds a
/// word of `OS_WORDS`, or a path through std's or core's `os` module, or through one of its
/// `OS_MODULES` as a group gives it. A module of the workspace that is named `os` is no OS's.
/// The patterns are spelled in parts, so that this file's own samples hold none of them.
fn os_names(text: &str) -> Vec<usize> {
    let line_of = |at: usize| text[..at].matches('\n').count() + 1;
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let os = concat!("os", "::");
    let mut lines: Vec<usize> = text
        .match_indices(os)
        .filter(|&(at, _)| {
            let before = &text[..at];
            let rest = &text[at + os.len()..];
            let crate_os = ["std::", "core::"]
                .iter()
                .any(|path| before.ends_with(path));
            let grouped = !before.ends_with(word)
                && !before.ends_with("::")
                && OS_MODULES.iter().any(|module| {
                    rest.strip_prefix(module)
                        .is_some_and(|after| !after.starts_with(word))
                });
            crate_os || grouped
        })
        .map(|(at, _)| line_of(at))
        .collect();
    for (at, _) in text.match_indices("cfg") {
        if text[..at].ends_with(word) {
            continue;
        }
        let rest = &text[at + 3..];
        let rest = rest
            .strip_prefix("_attr")
            .or_else(|| rest.strip_prefix('!'))
            .unwrap_or(rest);
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let mut depth = 1;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .map_or(rest.len(), |(end, _)| end);
        if rest[..end]
            .split(|c: char| !word(c))
            .any(|name| OS_WORDS.contains(&name))
        {
            lines.push(line_of(at));
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

#[test]
fn only_the_platform_module_names_an_os() {
    let sources = sources();
    let named: Vec<String> = sources
        .iter()
        .filter(|file| !file.path.starts_with(PLATFORM))
        .flat_map(|file| {
            os_names(&file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(named, Vec::<String>::new());
    // The module names one, so the walk reaches it and the check sees what it holds.
    let platform: Vec<&str> = sources
        .iter()
        .filter(|file| file.path.starts_with(PLATFORM) && !os_names(&file.text).is_empty())
        .map(|file| file.path.as_str())
        .collect();
    assert!(platform.contains(&"crates/campfire-store/src/platform/mod.rs"));
}

#[test]
fn the_platform_rule_check_finds_each_way_to_name_an_os() {
    let (cfg, os) = ("cfg", concat!("std", "::os"));
    for (text, lines) in [
        (format!("#[{cfg}(unix)]\nfn f() {{}}\n"), vec![1]),
        (format!("//! x\n#[{cfg}(all(test, windows))]\n"), vec![2]),
        (format!("let x = {cfg}!(target_os = \"macos\");\n"), vec![1]),
        (
            format!("#[{cfg}_attr(target_family = \"wasm\", path = \"w.rs\")]\n"),
            vec![1],
        ),
        (format!("use {os}::unix::fs;\n"), vec![1]),
        (format!("use std::{{fs, {}::windows}};\n", "os"), vec![1]),
        (format!("#[{cfg}(target_vendor = \"apple\")]\n"), vec![1]),
        (
            format!("#[{cfg}(not(any(unix, windows)))]\n#[{cfg}(unix)]\n"),
            vec![1, 2],
        ),
        // No OS: another condition, a word that only holds `cfg`, and an OS in a comment.
        (
            format!("#[{cfg}(test)]\n#[{cfg}(feature = \"internals\")]\n"),
            vec![],
        ),
        (
            format!(
                "let config = unix_time();\n// on windows\npub use crate::{}::Os;\n",
                "os"
            ),
            vec![],
        ),
    ] {
        assert_eq!(os_names(&text), lines, "{text}");
    }
}

#[test]
fn the_storage_rules_checks_fail_on_what_breaks_them() {
    // A list that misses a path, in the list it names and not in the other.
    let one = "disallowed-methods = [\n    { path = \"std::fs::write\", reason = \"r\" },\n]\n";
    let rules = ["std::fs::write", "std::fs::copy"];
    assert_eq!(
        unlisted(one, "disallowed-methods", &rules),
        ["std::fs::copy"]
    );
    assert_eq!(unlisted(one, "disallowed-types", &rules), rules);
    let [methods, types] = LINTS;
    let expect = |lint: &str| format!("#[expect({lint}, reason = \"r\")]\n");
    let inner = |lint: &str| format!("#!{}", &expect(lint)[1..]);
    let cases = [
        // An `expect` of either lint, on an item, a statement or a test module, anywhere but
        // `store`'s root and the schedule's settings.
        (
            "crates/x/src/a.rs",
            format!("{}fn f() {{}}\n", expect(methods)),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!("{}let x = 1;\n", expect(types)),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!("{}#[cfg(test)]\nmod tests;\n", expect(methods)),
            vec![1],
        ),
        ("crates/x/tests/mod.rs", inner(methods), vec![1]),
        (
            "crates/x/src/lib.rs",
            format!("//! x\n{}", inner(types)),
            vec![2],
        ),
        // `store`'s root, of both; the schedule's settings, of the methods' alone.
        (
            STORE_ROOT,
            format!("#![expect(\n    {methods},\n    {types},\n    reason = \"r\"\n)]\n"),
            vec![],
        ),
        (
            SCHEDULE_SETTINGS,
            format!("{}let x = 1;\n", expect(methods)),
            vec![],
        ),
        (
            SCHEDULE_SETTINGS,
            format!("{}let x = 1;\n", expect(types)),
            vec![1],
        ),
        (SCHEDULE_SETTINGS, inner(methods), vec![1]),
        // An `allow`, even in `store`; a mention is no attribute.
        (STORE_ROOT, format!("#![allow({methods})]\n"), vec![1]),
        (
            "crates/x/src/a.rs",
            format!("/// Mentions `{methods}`.\nfn f() {{}}\n"),
            vec![],
        ),
        // Turned off under a condition, or with another lint: both found.
        (
            "crates/x/src/a.rs",
            format!("#[cfg_attr(test, allow({types}))]\nfn f() {{}}\n"),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!("#[expect(clippy::other, {methods}, reason = \"r\")]\nmod io;\n"),
            vec![1],
        ),
        // A group that holds the lints, but a longer name that starts with one's.
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
        // A level that does not turn a lint off.
        (
            "crates/x/src/a.rs",
            format!("#[deny({types})]\nmod io;\n"),
            vec![],
        ),
    ];
    for (path, text, lines) in cases {
        assert_eq!(misplaced_exemptions(path, &text), lines, "{text}");
    }
}
