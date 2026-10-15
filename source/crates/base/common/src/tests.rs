use std::collections::{BTreeMap, BTreeSet};
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
                names.push(dependency(line));
            }
        }
    }
    names
}

/// The workspace's `source/` folder, three levels over this crate's manifest: `crates/base/common`.
fn source() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}

/// The dependencies of the crate at `dir`, under `source/crates/`, sorted.
fn crate_dependencies(dir: &str) -> Vec<String> {
    let manifest = read(
        source()
            .join("crates")
            .join(dir)
            .join("Cargo.toml")
            .to_str()
            .unwrap(),
    );
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
        crate_dependencies("base/common"),
        ["backtrace", "derive_more", "serde"]
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
    // second; a built module's crate is `campfire-<module>`, in a folder of `crates/` or `checks/`.
    let design = read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../docs/design/02-engine-core.md"
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
    let crates: Vec<String> = manifests().into_iter().map(|(name, _)| name).collect();
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
        dir: "base/store",
        lints: &LINTS,
        methods: &STORAGE_METHODS,
        types: &STORAGE_TYPES,
        dependencies: &[
            "libc",
            "signal-hook",
            "socket2",
            "tempfile",
            "thiserror",
            "windows",
            "zeroize",
        ],
        names: &["std::fs", "std::thread", "tempfile::"],
    },
    Proxy {
        dir: "base/codec",
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
        dir: "base/schedule",
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

/// The thread-local ban, the one ban an `expect` may exempt outside a proxy: a module whose state
/// an outside API keeps per thread, as a global allocator or tracing's default subscriber does,
/// holds a `thread_local!`, and names no other banned type, which its exemption would cover too.
const THREAD_LOCAL: &str = "std::thread::LocalKey";

/// What names a banned type in source, other than the thread-local one.
const TYPE_NAMES: [&str; 8] = [
    "std::fs",
    "tempfile::",
    "postcard::",
    "toml::",
    "serde_json::",
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
/// lint that proxy exempts, or of the types' lint at the head of a module that holds a
/// `thread_local!` and names no other banned type; never `allow`, a `cfg_attr`, a group that
/// holds the lint, or an attribute on an item or a statement, so a stale one fails and none hides.
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
            let proxied = PROXIES
                .iter()
                .any(|proxy| proxy.root() == path && proxy.lints.contains(name));
            let thread_state = *name == LINTS[1]
                && text.contains("thread_local!")
                && !TYPE_NAMES.iter().any(|banned| text.contains(banned));
            let allowed = inner && kind == "expect" && (proxied || thread_state);
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

/// A crate of the workspace: its folder's name, its package's name, and its manifest's text.
#[derive(Debug)]
struct Crate {
    folder: String,
    /// Its folder's path under `source/`, as `sources` names a file's.
    path: String,
    name: String,
    manifest: String,
}

/// The crates of the workspace, by their packages' names: each folder of `crates/` and `checks/`
/// that holds a `Cargo.toml`, at any depth of the groups, as a crate's folder may hold other
/// crates' folders beside its own `src/`.
fn crates() -> Vec<Crate> {
    let mut found = Vec::new();
    let mut folders = vec![source().join("crates"), source().join("checks")];
    while let Some(dir) = folders.pop() {
        let entries = DirEntries::read(&dir).unwrap();
        if entries
            .iter()
            .any(|inner: &DirEntry| inner.name == "Cargo.toml")
        {
            let manifest = read(dir.join("Cargo.toml").to_str().unwrap());
            // Joined by `/` on every OS, as a source file's path is.
            let path: Vec<&str> = dir
                .strip_prefix(source())
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str().unwrap())
                .collect();
            found.push(Crate {
                folder: dir.file_name().unwrap().to_str().unwrap().to_owned(),
                path: path.join("/"),
                name: package_name(&manifest).to_owned(),
                manifest,
            });
        }
        let own = ["src", "tests", "benches", "examples"];
        folders.extend(
            entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Dir)
                .filter(|entry| !own.iter().any(|name| entry.name == *name))
                .map(|entry| dir.join(&entry.name)),
        );
    }
    found.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The crates of the workspace, each by its package's name, with the text of its manifest.
fn manifests() -> Vec<(String, String)> {
    crates()
        .into_iter()
        .map(|krate| (krate.name, krate.manifest))
        .collect()
}

#[test]
fn each_crates_folder_is_its_package_name_without_the_prefix() {
    let misnamed: Vec<String> = crates()
        .into_iter()
        .filter(|krate| krate.name.strip_prefix("campfire-") != Some(krate.folder.as_str()))
        .map(|krate| format!("{} in {}", krate.name, krate.folder))
        .collect();
    assert_eq!(misnamed, Vec::<String>::new());
}

/// The package's name the manifest `manifest` gives.
fn package_name(manifest: &str) -> &str {
    let line = section(manifest, "package")
        .find(|line| line.starts_with("name ") || line.starts_with("name="))
        .expect("a manifest names its package");
    line.split('"')
        .nth(1)
        .expect("a package's name is a string")
}

/// The lines of `manifest`'s section `[name]`, past its header.
fn section<'a>(manifest: &'a str, name: &str) -> impl Iterator<Item = &'a str> {
    let header = format!("[{name}]");
    manifest
        .lines()
        .map(str::trim)
        .skip_while(move |line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// The crate a line of a manifest's dependencies names.
fn dependency(line: &str) -> &str {
    line.split(['.', ' ', '=']).next().unwrap()
}

/// The crates whose test infrastructure any crate's tests may take: `store`'s scratch and pinned
/// files, `log`'s check of the warnings, `common`'s allocation audit, `codec` for a type's encoded
/// form, and `math` for the numbers a crate's values hold (design 02, The capabilities' crates).
const TEST_INFRASTRUCTURE: [&str; 5] = [
    "campfire-codec",
    "campfire-common",
    "campfire-log",
    "campfire-math",
    "campfire-store",
];

/// The top crate of the session's and the network's tests: all of it is test code, its `tests/`
/// among it, and it depends on what they read.
const TOP_TEST_CRATE: &str = "campfire-harness";

/// The entries of `manifest`'s `internals` feature.
fn internals_entries(manifest: &str) -> Vec<&str> {
    let lines = section(manifest, "features").skip_while(|line| !line.starts_with("internals"));
    let mut entries = Vec::new();
    for line in lines {
        let list = line.trim_start_matches("internals = [");
        entries.extend(
            list.split(',')
                .map(|entry| entry.trim().trim_matches(['"', ']'])),
        );
        if line.ends_with(']') {
            break;
        }
    }
    entries.retain(|entry| !entry.is_empty());
    entries
}

/// What breaks the rule of the `internals` feature, by crate: a test build turns it on through
/// the dev-dependencies alone, so the code a test or a bench reaches has one gate,
/// `feature = "internals"`. A crate with the feature dev-depends on itself with it, and its
/// feature turns on the feature of each dependency that has it; each dev-dependency on a crate
/// with it turns it on, as a test of one crate uses another's helpers; and no crate has a `bench`
/// feature beside it. A dev-dependency on another crate of the workspace is test infrastructure,
/// or, of a crate with no feature of its own, as a binary, which cannot dev-depend on itself, a
/// dependency whose helpers its tests read.
fn internals_problems(manifests: &[(String, String)]) -> Vec<String> {
    let has = |manifest: &str, feature: &str| {
        section(manifest, "features").any(|line| line.starts_with(&format!("{feature} = ")))
    };
    let with_internals = |other: &str| {
        manifests
            .iter()
            .any(|(crate_name, manifest)| crate_name == other && has(manifest, "internals"))
    };
    let mut problems = Vec::new();
    for (name, manifest) in manifests {
        if has(manifest, "bench") {
            problems.push(format!("{name} has a bench feature"));
        }
        let own = has(manifest, "internals");
        let entries = internals_entries(manifest);
        let dependencies: Vec<&str> = section(manifest, "dependencies").collect();
        for line in &dependencies {
            let other = dependency(line);
            let passed = line.contains(r#""internals""#)
                || entries.contains(&format!("{other}/internals").as_str())
                || entries.contains(&format!("{other}?/internals").as_str());
            if own && with_internals(other) && !passed {
                problems.push(format!("{name}'s internals leaves {other}'s off"));
            }
        }
        for line in section(manifest, "dev-dependencies") {
            let other = dependency(line);
            if with_internals(other) && !line.contains(r#""internals""#) {
                problems.push(format!("{name} dev-depends on {other} with no internals"));
            }
            let workspace = manifests.iter().any(|(crate_name, _)| crate_name == other);
            let read = !own && dependencies.iter().any(|line| dependency(line) == other);
            let allowed = other == name || TEST_INFRASTRUCTURE.contains(&other) || read;
            if workspace && !allowed {
                problems.push(format!(
                    "{name} dev-depends on {other}, past its tests' rule"
                ));
            }
        }
        let itself = section(manifest, "dev-dependencies")
            .any(|line| dependency(line) == name && line.contains(r#""internals""#));
        if has(manifest, "internals") && !itself {
            problems.push(format!("{name} dev-depends on itself with no internals"));
        }
    }
    problems
}

/// The crates whose `internals` the build of `name`'s tests turns on: each dependency of any kind
/// that turns it on, and each crate whose `internals` one of those passes it on to. A weak entry,
/// `x?/internals`, counts for none, as it holds only where another feature turns `x` on.
fn internals_on(manifests: &[(String, String)], name: &str) -> Vec<String> {
    let manifest = |crate_name: &str| {
        manifests
            .iter()
            .find(|(other, _)| other == crate_name)
            .map(|(_, manifest)| manifest.as_str())
    };
    let own = manifest(name).unwrap();
    let mut on: Vec<String> = section(own, "dependencies")
        .chain(section(own, "dev-dependencies"))
        .filter(|line| line.contains(r#""internals""#))
        .map(|line| dependency(line).to_owned())
        .collect();
    let mut at = 0;
    while let Some(next) = on.get(at) {
        let passed: Vec<String> = manifest(next)
            .map(internals_entries)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| entry.strip_suffix("/internals"))
            .filter(|other| !other.ends_with('?') && !on.iter().any(|on| on == other))
            .map(str::to_owned)
            .collect();
        on.extend(passed);
        at += 1;
    }
    on
}

/// The crates of `manifests` but `name` whose `internals` `text`, code of `name`, reads, and its
/// tests' build leaves off: such code compiles only where another crate of the build turns it on.
fn internals_read_off(manifests: &[(String, String)], name: &str, text: &str) -> Vec<String> {
    let on = internals_on(manifests, name);
    manifests
        .iter()
        .map(|(other, _)| other)
        .filter(|other| *other != name && !on.contains(other))
        .filter(|other| {
            let path = format!("{}::internals", other.replace('-', "_"));
            text.match_indices(&path).any(|(at, _)| {
                !text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|before| before.is_alphanumeric() || before == '_')
            })
        })
        .cloned()
        .collect()
}

/// The lines of `text` that gate code on a test build or the `internals` feature, where the
/// feature alone gates it.
fn test_or_internals(text: &str) -> Vec<usize> {
    let gate = concat!("any(test, feature = ", "\"internals\"");
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(gate))
        .map(|(at, _)| at + 1)
        .collect()
}

#[test]
fn a_test_build_turns_internals_on_and_code_has_one_gate_for_it() {
    let manifests = manifests();
    assert_eq!(internals_problems(&manifests), Vec::<String>::new());
    let crates = crates();
    let read_off: Vec<String> = sources()
        .iter()
        .filter_map(|file| {
            let at = holder(&crates, &file.path)?;
            let off = internals_read_off(&manifests, &crates[at].name, &file.text);
            (!off.is_empty()).then(|| format!("{}: {}", file.path, off.join(", ")))
        })
        .collect();
    assert_eq!(read_off, Vec::<String>::new());
    let gated: Vec<String> = sources()
        .iter()
        .flat_map(|file| {
            test_or_internals(&file.text)
                .into_iter()
                .map(|line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(gated, Vec::<String>::new());
}

#[test]
fn the_internals_rule_names_each_crate_that_breaks_it() {
    // `a` has the feature and no dev-dependency on itself; `b` has it on itself, and on `a`
    // without it, and a `bench` feature beside it; `c` has none, and dev-depends on `b`, its
    // dependency, with it. `d` has the feature, leaves its dependency `b`'s off, and dev-depends on
    // it, on `c`, which is no dependency, and on `campfire-store`, test infrastructure.
    let crates = [
        (
            "a",
            "[features]\ninternals = []\n\n[dev-dependencies]\nb.workspace = true\n",
        ),
        (
            "b",
            concat!(
                "[features]\ninternals = []\nbench = []\n\n[dev-dependencies]\n",
                "a.workspace = true\nb = { workspace = true, features = [\"internals\"] }\n"
            ),
        ),
        (
            "c",
            concat!(
                "[dependencies]\nb.workspace = true\n\n[dev-dependencies]\n",
                "b = { workspace = true, features = [\"internals\"] }\n"
            ),
        ),
        (
            "d",
            concat!(
                "[features]\ninternals = [\n    \"a/internals\",\n    \"c?/internals\",\n]\n\n",
                "[dependencies]\nb.workspace = true\n\n[dev-dependencies]\n",
                "b = { workspace = true, features = [\"internals\"] }\nc.workspace = true\n",
                "campfire-store = { workspace = true, features = [\"internals\"] }\n",
                "d = { workspace = true, features = [\"internals\"] }\n"
            ),
        ),
    ]
    .map(|(name, manifest)| (name.to_owned(), manifest.to_owned()));
    // `a` turns no crate's on; `b` its own; `c` `b`'s; `d` its own, `a`'s through its own, `b`'s,
    // and `campfire-store`'s, which is not among them, and not `c`'s, which `d` passes on weakly.
    let reads = "x::internals::X; a::internals::A; b::internals::B; data::internals::D;";
    let weak = "c::internals::C;";
    let off = |name: &str| internals_read_off(&crates, name, reads);
    assert_eq!(off("a"), ["b"]);
    assert_eq!(off("b"), ["a"]);
    assert_eq!(off("c"), ["a"]);
    assert_eq!(off("d"), Vec::<String>::new());
    assert_eq!(internals_read_off(&crates, "d", weak), ["c"]);
    assert_eq!(
        internals_on(&crates, "d"),
        ["b", "campfire-store", "d", "a"]
    );
    assert_eq!(
        internals_problems(&crates),
        [
            "a dev-depends on b with no internals",
            "a dev-depends on b, past its tests' rule",
            "a dev-depends on itself with no internals",
            "b has a bench feature",
            "b dev-depends on a with no internals",
            "b dev-depends on a, past its tests' rule",
            "d's internals leaves b's off",
            "d dev-depends on b, past its tests' rule",
            "d dev-depends on c, past its tests' rule",
        ]
    );
    // The gate is spelled in parts, so this file holds none.
    let text = concat!(
        "#[cfg(feature = \"internals\")]\n#[cfg(any(test, feature = ",
        "\"internals\"))]\n"
    );
    assert_eq!(test_or_internals(text), [2]);
}

/// Each Rust source file of the workspace's crates and checks, by its path under `source/`,
/// with its text.
fn sources() -> Vec<SourceFile> {
    let mut read = Vec::new();
    for folder in ["crates", "checks"] {
        for file in SourceFiles::under(&source().join(folder)) {
            read.push(SourceFile {
                path: format!("{folder}/{}", file.path),
                ..file
            });
        }
    }
    read
}

#[test]
fn each_banned_call_goes_through_its_proxy_alone() {
    let clippy = read(source().join("clippy.toml").to_str().unwrap());
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
    assert_eq!(
        unlisted(&clippy, "disallowed-types", &[THREAD_LOCAL]),
        Vec::<&str>::new()
    );
    // A thread-local module's exemption covers every banned type, so each has a name it shows.
    let types = STORAGE_TYPES
        .iter()
        .chain(&CODEC_TYPES)
        .chain(&UNOWNED_TYPES);
    let unnamed: Vec<&str> = types
        .copied()
        .filter(|ty| !TYPE_NAMES.iter().any(|name| ty.contains(name)))
        .collect();
    assert_eq!(unnamed, Vec::<&str>::new());
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

/// The lines of `text` that re-export another crate, whole or in part: a `pub use`, of any
/// visibility, whose path starts with a name other than `crate`, `$crate`, `self` or `super`, and
/// a `pub extern crate`.
fn foreign_re_exports(text: &str) -> Vec<usize> {
    fn root(path: &str) -> &str {
        let path = path.trim_start_matches("::");
        let end = path
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(path.len());
        &path[..end]
    }
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let Some(rest) = line.trim_start().strip_prefix("pub") else {
                return false;
            };
            let rest = match rest.strip_prefix('(') {
                Some(scoped) => scoped.split_once(')').map_or("", |(_, rest)| rest),
                None => rest,
            };
            let rest = rest.trim_start();
            rest.starts_with("extern crate ")
                || rest
                    .strip_prefix("use ")
                    .is_some_and(|path| !["crate", "$crate", "self", "super"].contains(&root(path)))
        })
        .map(|(at, _)| at + 1)
        .collect()
}

#[test]
fn no_crate_re_exports_another_crate() {
    let found: Vec<String> = sources()
        .iter()
        .flat_map(|file| {
            foreign_re_exports(&file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // A whole crate, an item, a scoped one, an indented group and an extern crate are found; the
    // crate's own, a macro's own and a plain import are not.
    let sample = "pub use rhai;\npub use crate::a::B;\npub(crate) use ::toml::de::Error as E;\n    pub use other::{A, B};\npub use self::x;\nuse other::C;\npub use super::y;\npub extern crate rhai;\n        pub use $crate::Item;";
    assert_eq!(foreign_re_exports(sample), [1, 3, 4, 8]);
}

/// The required dependencies of `manifest` that `production`, its crate's production code, names
/// nowhere: a dependency that only code a gate holds uses is optional, and `internals` turns it on.
fn unnamed_dependencies<'a>(manifest: &'a str, production: &str) -> Vec<&'a str> {
    section(manifest, "dependencies")
        .filter(|line| !line.contains("optional = true"))
        .map(dependency)
        .filter(|name| !production.contains(&format!("{}::", name.replace('-', "_"))))
        .collect()
}

/// The index in `crates` of the crate that holds the source file at `path`, under `source/`: the
/// deepest whose folder holds it.
fn holder(crates: &[Crate], path: &str) -> Option<usize> {
    crates
        .iter()
        .enumerate()
        .filter(|(_, krate)| path.starts_with(&format!("{}/", krate.path)))
        .max_by_key(|(_, krate)| krate.path.len())
        .map(|(at, _)| at)
}

/// The production code of each of `crates`, in their order: each source's that no crate nested
/// in its folder holds; all of the top test crate's, as all of it is test code.
fn production_by_crate(crates: &[Crate]) -> Vec<String> {
    let mut code = vec![String::new(); crates.len()];
    for file in sources() {
        let Some(at) = holder(crates, &file.path) else {
            continue;
        };
        let krate = &crates[at];
        let production = if krate.name == TOP_TEST_CRATE {
            Some(file.text.clone())
        } else {
            file.production()
        };
        if let Some(production) = production {
            code[at].push_str(&production);
        }
    }
    code
}

#[test]
fn a_required_dependency_is_named_by_production_code() {
    let crates = crates();
    let mut found = Vec::new();
    for (krate, production) in crates.iter().zip(production_by_crate(&crates)) {
        for name in unnamed_dependencies(&krate.manifest, &production) {
            found.push(format!("{} {name}", krate.name));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // A plain and an inline dependency are found where no code names them; an optional one, and
    // one a path names, are not.
    let manifest = "[package]\nname = \"a\"\n\n[dependencies]\nb.workspace = true\nc-d = { workspace = true }\ne = { workspace = true, optional = true }\n";
    assert_eq!(unnamed_dependencies(manifest, ""), ["b", "c-d"]);
    assert_eq!(
        unnamed_dependencies(manifest, "use b::X; c_d::f();"),
        Vec::<&str>::new()
    );
}

/// The cycles of `imports`, a crate's top modules each with the top modules it imports: each
/// group of two or more modules that reach each other through their imports, sorted.
fn module_cycles(imports: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    let reach = |from: &str| {
        let mut seen = BTreeSet::new();
        let mut next = vec![from.to_owned()];
        while let Some(module) = next.pop() {
            for other in imports.get(&module).into_iter().flatten() {
                if seen.insert(other.clone()) {
                    next.push(other.clone());
                }
            }
        }
        seen
    };
    let reached: BTreeMap<&String, BTreeSet<String>> = imports
        .keys()
        .map(|module| (module, reach(module)))
        .collect();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    for (module, from) in &reached {
        let cycle: Vec<String> = from
            .iter()
            .filter(|other| {
                reached
                    .get(other)
                    .is_some_and(|back| back.contains(*module))
            })
            .cloned()
            .collect();
        if cycle.len() > 1 && !cycles.contains(&cycle) {
            cycles.push(cycle);
        }
    }
    cycles
}

/// Each path `code` names through `crate::`, by its segments, each path of a group its own:
/// `use crate::a::{b::X, c};` names `a::b::X` and `a::c`, and `crate::d::f()` names `d::f`. A
/// group's `self` names the path before it, and a glob that path. A macro's `$crate::` names no
/// module of the crate it is written in.
fn crate_paths(code: &str) -> Vec<Vec<&str>> {
    let mut paths = Vec::new();
    for (at, _) in code.match_indices("crate::") {
        if code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$') {
            continue;
        }
        use_tree(&code[at + "crate::".len()..], Vec::new(), &mut paths);
    }
    paths.retain(|path| !path.is_empty());
    paths
}

/// Reads the use tree at the start of `text`, under the segments `path`, and pushes each path
/// it names to `paths`; how much of `text` it read.
fn use_tree<'a>(text: &'a str, mut path: Vec<&'a str>, paths: &mut Vec<Vec<&'a str>>) -> usize {
    let skip = |at: usize| at + (text[at..].len() - text[at..].trim_start().len());
    let mut at = skip(0);
    loop {
        if text[at..].starts_with('{') {
            at = skip(at + 1);
            while !text[at..].starts_with('}') && at < text.len() {
                at = skip(at + use_tree(&text[at..], path.clone(), paths));
                if text[at..].starts_with(',') {
                    at = skip(at + 1);
                } else if !text[at..].starts_with('}') {
                    return at;
                }
            }
            return (at + 1).min(text.len());
        }
        let rest = &text[at..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        match &rest[..end] {
            "" | "self" => {}
            segment => path.push(segment),
        }
        at += end;
        if text[at..].starts_with("::") {
            at += "::".len();
        } else {
            paths.push(path);
            return at;
        }
    }
}

/// Each module of the crate at `src/` that has children, by its path, its children each with
/// the siblings its production code names: a child of `a::b` names its sibling `c` by a path
/// through `crate::a::b::c`. The crate's root is the module of no path.
fn sibling_imports(
    src: &str,
    sources: &[SourceFile],
) -> BTreeMap<Vec<String>, BTreeMap<String, BTreeSet<String>>> {
    let mut files = Vec::new();
    for file in sources {
        let Some(rest) = file.path.strip_prefix(&format!("{src}/")) else {
            continue;
        };
        let mut module: Vec<String> = rest
            .trim_end_matches(".rs")
            .split('/')
            .map(str::to_owned)
            .collect();
        if matches!(
            module.last().map(String::as_str),
            Some("mod" | "lib" | "main")
        ) {
            module.pop();
        }
        files.push((module, file));
    }
    let mut parents: BTreeMap<Vec<String>, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    for (module, _) in &files {
        for depth in 0..module.len() {
            let children = parents.entry(module[..depth].to_vec()).or_default();
            children.entry(module[depth].clone()).or_default();
        }
    }
    for (module, file) in &files {
        let Some(code) = file.production() else {
            continue;
        };
        let paths = crate_paths(&code);
        for depth in 0..module.len() {
            let (parent, child) = (&module[..depth], &module[depth]);
            let children = parents.get_mut(parent).expect("each parent is listed");
            for path in &paths {
                let through = path.len() > depth && path.iter().zip(parent).all(|(a, b)| a == b);
                let Some(&sibling) = path.get(depth).filter(|_| through) else {
                    continue;
                };
                if sibling != child && children.contains_key(sibling) {
                    let named = children.get_mut(child).expect("each child is listed");
                    named.insert(sibling.to_owned());
                }
            }
        }
    }
    parents
}

#[test]
fn no_crates_modules_import_each_other_in_a_cycle() {
    let sources = sources();
    let mut found = Vec::new();
    for krate in &crates() {
        let src = format!("{}/src", krate.path);
        for (parent, imports) in sibling_imports(&src, &sources) {
            let at: String = parent
                .iter()
                .flat_map(|module| ["::", module.as_str()])
                .collect();
            found.extend(
                module_cycles(&imports)
                    .into_iter()
                    .map(|cycle| format!("{}{at}: {}", krate.name, cycle.join(", "))),
            );
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // Three modules that import in a ring make one cycle; a module that only imports into it stays
    // out, as does a chain.
    let imports: BTreeMap<String, BTreeSet<String>> = [
        ("a", &["b"][..]),
        ("b", &["c"]),
        ("c", &["a"]),
        ("d", &["a"]),
        ("e", &["f"]),
        ("f", &[]),
    ]
    .into_iter()
    .map(|(module, named)| {
        (
            module.to_owned(),
            named.iter().map(|&name| name.to_owned()).collect(),
        )
    })
    .collect();
    assert_eq!(module_cycles(&imports), [["a", "b", "c"]]);
    // A path, a module, a group with a nested group, `self` and a trailing comma, a glob, a call
    // and a macro's own crate.
    let code = "use crate::a::X;\nuse crate::b;\nuse crate::{c::{self, Y, z::Z}, d,\n};\nuse crate::e::*;\ncrate::f::g();\n$crate::h::H";
    let paths: Vec<String> = crate_paths(code)
        .iter()
        .map(|path| path.join("::"))
        .collect();
    assert_eq!(
        paths,
        ["a::X", "b", "c", "c::Y", "c::z::Z", "d", "e", "f::g"]
    );
    // Under the crate's `p`, its child `q` names `r` through a group, and `r` names `q` through a
    // path: a cycle of `p`'s children. The root's `p` naming itself is none.
    let file = |path: &str, text: &str| SourceFile {
        path: format!("src/{path}"),
        text: text.to_owned(),
        gated: false,
    };
    let files = [
        file("lib.rs", "mod p;"),
        file("p/mod.rs", "use crate::p::q::Q;"),
        file("p/q.rs", "use crate::p::{r::R, s};"),
        file("p/r.rs", "use crate::p::q::Q;"),
        file("p/s/mod.rs", ""),
    ];
    let imports = sibling_imports("src", &files);
    let under_p = &imports[&vec!["p".to_owned()]];
    assert_eq!(module_cycles(under_p), [["q", "r"]]);
    assert!(module_cycles(&imports[&Vec::new()]).is_empty());
}

/// The one module whose code may differ by OS (design 02, Platform).
const PLATFORM: &str = "crates/base/store/src/platform/";

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
    let mut platform = sources
        .iter()
        .filter(|file| file.path.starts_with(PLATFORM) && !os_names(&file.text).is_empty())
        .map(|file| file.path.as_str());
    assert!(platform.any(|path| path == "crates/base/store/src/platform/mod.rs"));
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
            "crates/base/store/src/lib.rs",
            format!("#![expect(\n    {methods},\n    {types},\n    reason = \"r\"\n)]\n"),
            vec![],
        ),
        ("crates/base/codec/src/lib.rs", inner(types), vec![]),
        ("crates/base/schedule/src/lib.rs", inner(types), vec![1]),
        ("crates/base/schedule/src/lib.rs", inner(methods), vec![]),
        ("crates/base/codec/src/binary.rs", inner(methods), vec![1]),
        (
            "crates/sim/sim/src/sim_update/mod.rs",
            format!("{}let x = 1;\n", expect(methods)),
            vec![1],
        ),
        // A module that holds a thread-local: an `expect` of the types' lint at its head, while it
        // names no other banned type; of the methods' lint, or on an item, it is found.
        (
            "crates/x/src/a.rs",
            format!("{}thread_local! {{}}\n", inner(types)),
            vec![],
        ),
        (
            "crates/x/src/a.rs",
            format!("{}thread_local! {{}}\n", inner(methods)),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!("{}thread_local! {{}}\n", expect(types)),
            vec![1],
        ),
        (
            "crates/x/src/a.rs",
            format!(
                "{}use std::collections::HashMap;\nthread_local! {{}}\n",
                inner(types)
            ),
            vec![1],
        ),
        // An `allow`, even in `store`; a mention is no attribute.
        (
            "crates/base/store/src/lib.rs",
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
    let production: Vec<(&str, String)> = sources
        .iter()
        .filter_map(|file| Some((file.path.as_str(), file.production()?)))
        .collect();
    let mut found = Vec::new();
    for (path, code) in &production {
        let free = world_first(code)
            .into_iter()
            .filter(|f| f.visible && !f.name.contains("::"));
        for function in free {
            let valued = production
                .iter()
                .any(|(_, text)| named_as_value(text, &function.name));
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
    let own = "crates/base/common/src/tests.rs";
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
