use crate::workspace::{read, source, sources};

/// A proxy crate: the one crate that makes the calls of its group of `clippy.toml`'s bans, through
/// an `expect` of the lints at its crate root alone, and gives every other crate what it needs
/// of them. A crate's exemption covers each banned call of the lints, of any group, so its
/// dependencies are listed and its source names no other group's call.
#[derive(Debug)]
pub(crate) struct Proxy {
    /// Its directory under `source/crates/`.
    pub(crate) dir: &'static str,
    /// The lints its root exempts.
    pub(crate) lints: &'static [&'static str],
    /// Calls and types of its group that `clippy.toml` must keep banning.
    pub(crate) methods: &'static [&'static str],
    pub(crate) types: &'static [&'static str],
    /// Every crate it depends on, sorted.
    pub(crate) dependencies: &'static [&'static str],
    /// What names its group's calls in source, which no other proxy's source holds.
    pub(crate) names: &'static [&'static str],
}

impl Proxy {
    /// Its crate root, under `source/`.
    fn root(&self) -> String {
        format!("crates/{}/src/lib.rs", self.dir)
    }
}

/// The calls only `store` makes (02-engine-core.md, Storage): those that read, list, check, write,
/// rename or remove a file or make a directory, `Path`'s probes, and those that start a thread.
const STORAGE_METHODS: [&str; 37] = [
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
    "tempfile::tempdir_in",
    "tempfile::tempfile_in",
    "std::thread::spawn",
    "std::thread::scope",
    "std::thread::Builder::spawn",
    "std::thread::Builder::spawn_scoped",
    "std::thread::Builder::spawn_unchecked",
];

/// The types only `store` holds, each banned whole so every method of it is.
const STORAGE_TYPES: [&str; 10] = [
    "std::fs::File",
    "std::fs::OpenOptions",
    "std::fs::DirBuilder",
    "std::fs::ReadDir",
    "std::fs::DirEntry",
    "std::fs::Metadata",
    "tempfile::TempDir",
    "tempfile::NamedTempFile",
    "tempfile::Builder",
    "tempfile::SpooledTempFile",
];

/// The calls only `codec` makes (02-engine-core.md, Serialization): each format's own encodes and
/// decodes, and replicon's helpers over postcard.
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
    "serde_json::from_value",
    "serde_json::to_value",
    "toml::Value::try_into",
    "toml::map::Map::try_into",
    "bevy_replicon::postcard_utils::to_extend_mut",
    "bevy_replicon::postcard_utils::from_buf",
];

/// The types only `codec` holds, each banned whole so every method of it is: a value type among
/// them, which is a deserializer of its own.
const CODEC_TYPES: [&str; 9] = [
    "postcard::Serializer",
    "postcard::Deserializer",
    "toml::Serializer",
    "toml::Deserializer",
    "serde_json::Serializer",
    "serde_json::Deserializer",
    "serde_json::Value",
    "toml::Value",
    "toml::Table",
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
pub(crate) const PROXIES: [Proxy; 3] = [
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
pub(crate) fn foreign_names(proxy: &Proxy, text: &str) -> Vec<&'static str> {
    PROXIES
        .iter()
        .filter(|other| other.dir != proxy.dir)
        .flat_map(|other| other.names.iter())
        .chain(&UNOWNED_NAMES)
        .copied()
        .filter(|name| text.contains(name))
        .collect()
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

/// The crates whose code a tick runs, where a removal reaches its reader through `Removals` alone
/// (02-engine-core.md, A tick's removals are the tick's).
const TICK_CRATES: [&str; 2] = ["crates/sim/", "crates/capabilities/"];

/// The line of each place where the code of `text` that every build holds, not a comment nor a
/// test module, names Bevy's `RemovedComponents`.
fn app_removals(text: &str) -> Vec<usize> {
    let built = text
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(text);
    (1..)
        .zip(built.lines())
        .filter(|(_, line)| {
            let code = line.split_once("//").map_or(*line, |(code, _)| code);
            code.contains("RemovedComponents")
        })
        .map(|(at, _)| at)
        .collect()
}

#[test]
fn no_tick_reads_removals_that_age_with_the_app() {
    let found: Vec<String> = sources()
        .iter()
        .filter(|file| file.gate.is_none())
        .filter(|file| TICK_CRATES.iter().any(|dir| file.path.starts_with(dir)))
        .flat_map(|file| {
            app_removals(&file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // A reader in code is found; a mention in a comment, and a test's reader, are not.
    let sample = concat!(
        "/// Unlike `RemovedComponents`.\n",
        "fn f(gone: RemovedComponents<'_, '_, A>) {} // here\n",
        "\n#[cfg(test)]\nmod tests {\n",
        "    fn g(gone: RemovedComponents<'_, '_, A>) {}\n",
        "}\n",
    );
    assert_eq!(app_removals(sample), [2]);
}
