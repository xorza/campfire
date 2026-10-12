use std::panic;
use std::path::Path;

use campfire_store::internals::{SourceFile, SourceFiles};
use campfire_store::{DirEntries, DirEntry, EntryKind, InputFile};

/// The most bytes a test reads of a file of the workspace's own: far past any it holds.
const FILE_LEN: usize = 1 << 20;

/// The text of the workspace's file at `path`.
fn read(path: &str) -> String {
    InputFile::read_text(Path::new(path), FILE_LEN).unwrap()
}

/// The names of the crates the manifest `manifest` depends on, its target-specific dependencies
/// among them, in the order it lists them. Read by hand, as the crate has no TOML parser: only the
/// known sections may appear, so a dependency in another form, such as `[dependencies.name]` or a
/// dotted key before the first section, fails instead of passing unseen.
fn dependencies(manifest: &str) -> Vec<&str> {
    let mut section = "";
    let mut names = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            section = header.strip_suffix(']').unwrap();
            let target =
                section.starts_with("target.'cfg(") && section.ends_with(")'.dependencies");
            let known = [
                "package",
                "features",
                "dependencies",
                "dev-dependencies",
                "[test]",
                "lints",
            ];
            assert!(known.contains(&section) || target, "section [{section}]");
            if target {
                section = "dependencies";
            }
        } else {
            assert!(!section.is_empty(), "{line:?} before the first section");
            if section == "dependencies" {
                names.push(line.split(['.', ' ', '=']).next().unwrap());
            }
        }
    }
    names
}

/// The dependencies of the crate at `dir`, under `source/crates/`, sorted.
fn crate_dependencies(dir: &str) -> Vec<String> {
    let manifest = read(&format!(
        "{}/../{dir}/Cargo.toml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut names: Vec<String> = dependencies(&manifest)
        .into_iter()
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn common_and_each_proxy_depend_on_their_listed_crates_alone() {
    // `common` holds plain values, so it needs serde and its derives alone, and `backtrace` for
    // the allocation audit, which only a test build turns on; a proxy reaches only its own
    // group's crates, so its exemption covers no call of another group.
    assert_eq!(
        crate_dependencies("campfire-common"),
        ["backtrace", "derive_more", "serde", "thiserror"]
    );
    let manifest = read(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
    assert!(
        manifest
            .lines()
            .any(|line| line.starts_with("backtrace = ") && line.contains("optional = true"))
    );
    assert!(manifest.contains(r#"internals = ["dep:backtrace"]"#));
    for proxy in &PROXIES {
        assert_eq!(
            crate_dependencies(proxy.dir),
            proxy.dependencies,
            "{}",
            proxy.dir
        );
    }
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

/// A proxy crate: the one crate that makes the calls of its group of `clippy.toml`'s bans, through
/// an `expect` of the lints at its crate root alone, and gives every other crate what it needs
/// of them. A crate's exemption covers each banned call of the lints, of any group, so its
/// dependencies are listed and its source names no other group's call.
#[derive(Debug)]
struct Proxy {
    /// Its directory under `source/crates/`.
    dir: &'static str,
    /// The lints its root exempts.
    lints: &'static [&'static str],
    /// Calls and types of its group that `clippy.toml` must keep banning.
    methods: &'static [&'static str],
    types: &'static [&'static str],
    /// Every crate it depends on, sorted.
    dependencies: &'static [&'static str],
    /// What names its group's calls in source, which no other proxy's source holds.
    names: &'static [&'static str],
}

impl Proxy {
    /// Its crate root, under `source/`.
    fn root(&self) -> String {
        format!("crates/{}/src/lib.rs", self.dir)
    }
}

/// The calls only `store` makes (design 02, Storage): those that read, list, check, write,
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

/// The calls only `codec` makes (design 02, Serialization): each format's own encodes and
/// decodes, and replicon's helpers over postcard.
const CODEC_METHODS: [&str; 20] = [
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

/// The calls only `schedule` makes: those that set a schedule's build settings, which can switch
/// off the ambiguity and hierarchy checks.
const SCHEDULE_METHODS: [&str; 2] = [
    "bevy_ecs::schedule::Schedule::set_build_settings",
    "bevy_ecs::schedule::Schedules::configure_schedules",
];

/// The lints of the bans, and the groups that hold them, named in parts so that this file's own
/// samples hold no attribute of them.
const LINTS: [&str; 2] = [
    concat!("clippy::", "disallowed_methods"),
    concat!("clippy::", "disallowed_types"),
];
const GROUPS: [&str; 2] = [concat!("clippy::", "style"), concat!("clippy::", "all")];

/// Every proxy crate, each the owner of one group of the bans.
const PROXIES: [Proxy; 3] = [
    Proxy {
        dir: "campfire-store",
        lints: &LINTS,
        methods: &STORAGE_METHODS,
        types: &STORAGE_TYPES,
        dependencies: &[
            "libc",
            "signal-hook",
            "tempfile",
            "thiserror",
            "windows",
            "zeroize",
        ],
        names: &["std::fs", "std::thread", "tempfile::"],
    },
    Proxy {
        dir: "campfire-codec",
        lints: &LINTS,
        methods: &CODEC_METHODS,
        types: &CODEC_TYPES,
        dependencies: &[
            "arrayvec",
            "postcard",
            "serde",
            "serde_json",
            "thiserror",
            "toml",
        ],
        names: &["postcard::", "serde_json::", "toml::", "postcard_utils"],
    },
    Proxy {
        dir: "campfire-schedule",
        lints: &[LINTS[0]],
        methods: &SCHEDULE_METHODS,
        types: &[],
        dependencies: &["bevy_ecs"],
        names: &["set_build_settings", "configure_schedules"],
    },
];

/// The bans with no proxy, which nothing may exempt: each lets two conflicting sim systems run in
/// either order, takes Lightyear's own encoding past the codec, or seeds a map at random.
const UNOWNED_METHODS: [&str; 8] = [
    "bevy_ecs::schedule::IntoScheduleConfigs::ambiguous_with",
    "bevy_ecs::schedule::Schedule::ignore_ambiguity",
    "bevy_ecs::schedule::Schedules::allow_ambiguous_component",
    "bevy_ecs::world::World::allow_ambiguous_resource",
    "lightyear_messages::registry::AppMessageExt::register_message",
    "lightyear_replication::registry::replication::AppComponentExt::register_component",
    "lightyear_replication::registry::replication::ComponentRegistration::replicate",
    "lightyear_replication::registry::replication::ComponentRegistration::replicate_once",
];
const UNOWNED_TYPES: [&str; 3] = [
    "std::collections::HashMap",
    "std::collections::HashSet",
    "std::hash::RandomState",
];

/// What names a call of no proxy's group in source, which no proxy's source holds, as its
/// exemption would cover it.
const UNOWNED_NAMES: [&str; 9] = [
    "ambiguous_with",
    "ignore_ambiguity",
    "allow_ambiguous",
    "register_message",
    "register_component",
    ".replicate",
    "HashMap",
    "HashSet",
    "RandomState",
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

/// The lines of `text`, the source at `path` under `source/`, whose attribute exempts code from
/// either lint other than as the proxies let it: only an `expect`, at a proxy's crate root, of a
/// lint that proxy exempts; never `allow`, a `cfg_attr`, a group that holds the lint, or an
/// attribute on an item or a statement, so a stale one fails and none hides.
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
            let allowed = inner
                && kind == "expect"
                && PROXIES
                    .iter()
                    .any(|proxy| proxy.root() == path && proxy.lints.contains(name));
            if !allowed {
                misplaced.push(line_of(at));
            }
        }
    }
    misplaced.sort_unstable();
    misplaced
}

/// What of another group's names, another proxy's or no proxy's, `text`, a source of `proxy`'s,
/// holds.
fn foreign_names(proxy: &Proxy, text: &str) -> Vec<&'static str> {
    PROXIES
        .iter()
        .filter(|other| other.dir != proxy.dir)
        .flat_map(|other| other.names.iter())
        .chain(&UNOWNED_NAMES)
        .copied()
        .filter(|name| text.contains(name))
        .collect()
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
fn each_banned_call_goes_through_its_proxy_alone() {
    let clippy = read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../clippy.toml"));
    let sources = sources();
    for proxy in &PROXIES {
        let methods = unlisted(&clippy, "disallowed-methods", proxy.methods);
        assert_eq!(methods, Vec::<&str>::new(), "{}", proxy.dir);
        let types = unlisted(&clippy, "disallowed-types", proxy.types);
        assert_eq!(types, Vec::<&str>::new(), "{}", proxy.dir);
        assert!(sources.iter().any(|file| file.path == proxy.root()));
        let own = format!("crates/{}/", proxy.dir);
        let foreign: Vec<String> = sources
            .iter()
            .filter(|file| file.path.starts_with(&own))
            .flat_map(|file| {
                foreign_names(proxy, &file.text)
                    .into_iter()
                    .map(move |name| format!("{}: {name}", file.path))
            })
            .collect();
        assert_eq!(foreign, Vec::<String>::new());
    }
    let methods = unlisted(&clippy, "disallowed-methods", &UNOWNED_METHODS);
    assert_eq!(methods, Vec::<&str>::new());
    let types = unlisted(&clippy, "disallowed-types", &UNOWNED_TYPES);
    assert_eq!(types, Vec::<&str>::new());
    let misplaced: Vec<String> = sources
        .iter()
        .flat_map(|file| {
            misplaced_exemptions(&file.path, &file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(misplaced, Vec::<String>::new());
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
fn the_ban_checks_fail_on_what_breaks_them() {
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
        // An `expect` of either lint, on an item, a statement or a test module, outside the
        // proxies' roots.
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
        // A proxy's root, of the lints it exempts: `store`'s and `codec`'s of both, `schedule`'s
        // of the methods' alone; another of its files, or a statement anywhere, is found.
        (
            "crates/campfire-store/src/lib.rs",
            format!("#![expect(\n    {methods},\n    {types},\n    reason = \"r\"\n)]\n"),
            vec![],
        ),
        ("crates/campfire-codec/src/lib.rs", inner(types), vec![]),
        ("crates/campfire-schedule/src/lib.rs", inner(types), vec![1]),
        (
            "crates/campfire-schedule/src/lib.rs",
            inner(methods),
            vec![],
        ),
        (
            "crates/campfire-codec/src/binary.rs",
            inner(methods),
            vec![1],
        ),
        (
            "crates/campfire-sim/src/sim_update/mod.rs",
            format!("{}let x = 1;\n", expect(methods)),
            vec![1],
        ),
        // An `allow`, even in `store`; a mention is no attribute.
        (
            "crates/campfire-store/src/lib.rs",
            format!("#![allow({methods})]\n"),
            vec![1],
        ),
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

#[test]
fn the_dependency_and_name_checks_fail_on_what_breaks_them() {
    // A manifest's dependencies, a target's among them, and not its dev-dependencies or its
    // features; a section the reader does not know fails it. The target's condition is spelled
    // in parts, so this file names no OS.
    let manifest = concat!(
        "[package]\nname = \"x\"\n\n[features]\ni = [\"dep:t\"]\n\n[dependencies]\n",
        "a.workspace = true\nt = { workspace = true, optional = true }\n\n",
        "[target.'cfg(",
        "un",
        "ix)'.dependencies]\nl.workspace = true\n\n[dev-dependencies]\nd.workspace = true\n"
    );
    assert_eq!(dependencies(manifest), ["a", "t", "l"]);
    let dotted = panic::catch_unwind(|| dependencies("[dependencies.a]\nversion = \"1\"\n"));
    assert!(dotted.is_err());
    // A proxy's source that names another proxy's call or one of no proxy's, but not its own.
    let [store, codec, _] = &PROXIES;
    let text = "use std::fs::File;\nlet map: HashMap<u8, u8>;\nlet n = 1;";
    assert_eq!(foreign_names(codec, text), ["std::fs", "HashMap"]);
    assert_eq!(
        foreign_names(store, "use std::fs::File;"),
        Vec::<&str>::new()
    );
}

/// A function whose first parameter is a world: its line, its name with its owner's, as
/// `Owner::name`, or alone for a free function, and whether code past its module may call it.
#[derive(Debug, PartialEq, Eq)]
struct WorldFirst {
    line: usize,
    name: String,
    visible: bool,
}

/// The end of the bracketed run that opens at `text[at]`, past its closing bracket.
fn bracket_end(text: &str, at: usize, open: char, close: char) -> usize {
    let mut depth = 0;
    for (offset, c) in text[at..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return at + offset + 1;
            }
        }
    }
    text.len()
}

/// The type an `impl` line of `line` gives methods to, none for a trait's impl.
fn impl_owner(line: &str) -> Option<&str> {
    let mut rest = line.trim_start().strip_prefix("impl")?;
    if rest.starts_with('<') {
        rest = &rest[bracket_end(rest, 0, '<', '>')..];
    }
    let rest = rest.trim_start();
    if rest.contains(" for ") {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Each function of `text` whose first parameter is a world, a reference to a `World`.
fn world_first(text: &str) -> Vec<WorldFirst> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (at, _) in text.match_indices("fn ") {
        let line_start = text[..at].rfind('\n').map_or(0, |end| end + 1);
        let head = &text[line_start..at];
        let modifiers = head.trim_start();
        let words = modifiers
            .split_whitespace()
            .all(|word| word.starts_with("pub") || word == "const");
        if !words || text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = &text[at + 3..];
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        let mut params = at + 3 + name_end;
        if text[params..].starts_with('<') {
            params = bracket_end(text, params, '<', '>');
        }
        if !text[params..].starts_with('(') {
            continue;
        }
        let close = bracket_end(text, params, '(', ')') - 1;
        let list = &text[params + 1..close];
        let first = list.split(',').next().unwrap_or("");
        let Some((_, kind)) = first.split_once(':') else {
            continue;
        };
        let Some(referent) = kind.trim().strip_prefix('&') else {
            continue;
        };
        let referent = referent.trim_start();
        let referent = referent.strip_prefix('\'').map_or(referent, |lifetime| {
            lifetime
                .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
                .trim_start()
        });
        let referent = referent
            .strip_prefix("mut ")
            .map_or(referent, str::trim_start);
        if referent != "World" {
            continue;
        }
        let line = text[..at].matches('\n').count();
        let indent = head.len() - modifiers.len();
        let owner = lines[..line].iter().rev().find_map(|above| {
            let code = above.trim_start();
            let at_impl = indent >= 4 && above.len() - code.len() == indent - 4;
            (at_impl && code.starts_with("impl")).then(|| impl_owner(above))
        });
        let name = match owner {
            Some(None) => continue,
            Some(Some(owner)) => format!("{owner}::{name}"),
            None => name.to_owned(),
        };
        let visible = modifiers.starts_with("pub") && !modifiers.starts_with("pub(super)");
        found.push(WorldFirst {
            line: line + 1,
            name,
            visible,
        });
    }
    found
}

/// Whether `text` names the function `name`, as `Owner::name` or alone, as a value: Bevy
/// registers a system, an observer, a hook or a run condition by value, and a table holds a
/// function so.
fn named_as_value(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].trim_end();
        let after = text[at + name.len()..].trim_start();
        let whole = !text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':')
            && !text[at + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
        let called = after.starts_with(['(', ':', '<']);
        let listed = before.ends_with(['(', ',', '[']) || name.contains("::");
        let line = &text[text[..at].rfind('\n').map_or(0, |end| end + 1)..at];
        let comment = line.trim_start().starts_with("//");
        whole && !called && listed && !comment
    })
}

#[test]
fn a_free_function_that_takes_a_world_first_is_a_system() {
    // Design 02's world rule: what acts on the match's content is a function of its module's
    // type, and a value made from the world a constructor of its own. A free function past its
    // module that takes the world first is a system or a function a table holds, named as a value.
    let sources = sources();
    let production: Vec<(&str, &str)> = sources
        .iter()
        .filter_map(|file| Some((file.path.as_str(), file.production()?)))
        .collect();
    let mut found = Vec::new();
    for &(path, code) in &production {
        let free = world_first(code)
            .into_iter()
            .filter(|f| f.visible && !f.name.contains("::"));
        for function in free {
            let valued = production
                .iter()
                .any(|&(_, text)| named_as_value(text, &function.name));
            if !valued {
                found.push(format!("{path}:{} {}", function.line, function.name));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

/// Each trait `text` declares that the world rule refuses, by its line and name: a test trait
/// on the world, `<Module>WorldTest`; a crate's internals trait on it,
/// `<Module>WorldCrateInternals`; and a module's trait on `Match`, `<Module>Match` or
/// `<Module>MatchInternals`, as a match a module builds comes from the module's type.
fn refused_traits(text: &str) -> Vec<(usize, &str)> {
    const REFUSED: [&str; 4] = [
        "WorldTest",
        "WorldCrateInternals",
        "Match",
        "MatchInternals",
    ];
    let mut found = Vec::new();
    for (line, code) in text.lines().enumerate() {
        let mut code = code.trim_start();
        if let Some(rest) = code.strip_prefix("pub") {
            let rest = rest.strip_prefix('(').map_or(rest, |inner| {
                inner.split_once(')').map_or(inner, |(_, after)| after)
            });
            code = rest.trim_start();
        }
        let Some(rest) = code.strip_prefix("trait ") else {
            continue;
        };
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let refused = REFUSED
            .iter()
            .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix));
        if refused {
            found.push((line + 1, name));
        }
    }
    found
}

#[test]
fn no_trait_is_a_test_trait_on_the_world_or_a_modules_trait_on_a_match() {
    // Design 02's world rule: a world trait holds the whole match and the fast access to a
    // module's systems, for other crates, its own crate, or the tests and benches of any crate;
    // a test's verbs belong to its fixture or its capability's type, and a match a module builds
    // to the module's type. Test code counts, as these traits lived in it; this file holds a
    // source of each form for the check's own test.
    let mut found = Vec::new();
    let own = "crates/campfire-common/src/tests.rs";
    for file in sources().into_iter().filter(|file| file.path != own) {
        for (line, name) in refused_traits(&file.text) {
            found.push(format!("{}:{line} {name}", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_world_rule_finds_each_trait_it_refuses() {
    // Each refused form, at any visibility; the world traits it keeps, a trait named `Match`
    // alone and a type of such a name are none.
    let text = "pub trait UnitsWorldTest {}
\
        pub(crate) trait ActionsWorldCrateInternals {}
\
        \x20   trait SessionMatch: Sized {}
\
        pub(super) trait HarnessMatchInternals {}
\
        pub trait SimWorld {}
pub(crate) trait PlayersWorldCrate {}
\
        pub trait SimWorldInternals {}
trait Match {}
pub struct ScriptedMatch;
";
    assert_eq!(
        refused_traits(text),
        [
            (1, "UnitsWorldTest"),
            (2, "ActionsWorldCrateInternals"),
            (3, "SessionMatch"),
            (4, "HarnessMatchInternals"),
        ]
    );
}

#[test]
fn the_world_rule_finds_each_function_that_takes_a_world_first() {
    // A free function and methods, of each form of a reference to a world, their visibility
    // read from their modifiers; a method of a trait's impl, one that takes the world later,
    // and one that takes a world by value are none.
    let text = "pub fn a(world: &World) {}\n\
        impl<'w> Owner<'w> {\n    pub(crate) const fn b<T>(\n        w: &'w mut World,\n    ) {}\n\
        \x20   pub(super) fn c(world: &mut World, x: u8) {}\n    fn d(x: u8, world: &World) {}\n}\n\
        impl Trait for Owner {\n    fn e(world: &World) {}\n}\n\
        fn f(world: World) {}\nfn g(world: &'a World) {}\n";
    let found = world_first(text);
    let expected = [
        (1, "a", true),
        (3, "Owner::b", true),
        (6, "Owner::c", false),
        (13, "g", false),
    ];
    let expected: Vec<WorldFirst> = expected
        .into_iter()
        .map(|(line, name, visible)| WorldFirst {
            line,
            name: name.to_owned(),
            visible,
        })
        .collect();
    assert_eq!(found, expected);
    // A name passed or listed as a value counts; a call, a path through it, a longer name and a
    // comment's mention do not.
    assert!(named_as_value(".add_systems(Owner::run)", "Owner::run"));
    assert!(named_as_value("[a, run, b]", "run"));
    assert!(!named_as_value("Owner::run(world)", "Owner::run"));
    assert!(!named_as_value("Owner::run::<T>(world)", "Owner::run"));
    assert!(!named_as_value(
        ".add_systems(Owner::running)",
        "Owner::run"
    ));
    assert!(!named_as_value("let x = run;", "run"));
    assert!(!named_as_value("/// As `Owner::run` does.", "Owner::run"));
}
