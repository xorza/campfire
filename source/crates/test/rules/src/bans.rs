use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;

use crate::workspace::{crates, manifests, read, sim_closure, source, sources};

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
    /// The crates under `source/crates/` whose own `clippy.toml` bans its group, as Clippy reads
    /// the nearest `clippy.toml` alone; none when the workspace's bans it everywhere.
    pub(crate) crates: &'static [&'static str],
}

impl Proxy {
    /// Its crate root, under `source/`.
    fn root(&self) -> String {
        format!("crates/{}/src/lib.rs", self.dir)
    }
}

/// The calls only `store` makes (02-engine-core.md, Storage): those that read, list, check, write,
/// rename or remove a file or make a directory, and `Path`'s probes.
const STORAGE_METHODS: [&str; 32] = [
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
];

/// The calls only `threads` makes (02-engine-core.md, Threads are a crate of their own): those that
/// start a thread.
const THREAD_METHODS: [&str; 5] = [
    "std::thread::spawn",
    "std::thread::scope",
    "std::thread::Builder::spawn",
    "std::thread::Builder::spawn_scoped",
    "std::thread::Builder::spawn_unchecked",
];

/// The types only `app-threads` names (02-engine-core.md, An app's threads are one plan): the
/// plugin and the options that size Bevy's task pools.
const APP_THREAD_TYPES: [&str; 2] = ["bevy_app::TaskPoolPlugin", "bevy_app::TaskPoolOptions"];

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
/// off the ambiguity and hierarchy checks, and its executor, which can run its systems on other
/// threads.
const SCHEDULE_METHODS: [&str; 3] = [
    "bevy_ecs::schedule::Schedule::set_build_settings",
    "bevy_ecs::schedule::Schedules::configure_schedules",
    "bevy_ecs::schedule::Schedule::set_executor",
];

/// The despawns only `campfire-net-despawns` makes in the network stack's replication crates
/// (13-net-stack/replication.md, The server alone ends a replicated entity's life on the
/// receiver): each of Bevy's despawns, which takes each child the server still owns.
const DESPAWN_METHODS: [&str; 19] = [
    "bevy_ecs::system::Commands::despawn_all",
    "bevy_ecs::system::Commands::despawn_all_where",
    "bevy_ecs::system::EntityCommands::despawn",
    "bevy_ecs::system::EntityCommands::despawn_children",
    "bevy_ecs::system::EntityCommands::despawn_related",
    "bevy_ecs::system::EntityCommands::try_despawn",
    "bevy_ecs::system::command::despawn_all",
    "bevy_ecs::system::command::despawn_all_where",
    "bevy_ecs::system::entity_command::despawn",
    "bevy_ecs::world::EntityWorldMut::despawn",
    "bevy_ecs::world::EntityWorldMut::despawn_children",
    "bevy_ecs::world::EntityWorldMut::despawn_no_free",
    "bevy_ecs::world::EntityWorldMut::despawn_related",
    "bevy_ecs::world::World::despawn",
    "bevy_ecs::world::World::despawn_all",
    "bevy_ecs::world::World::despawn_all_where",
    "bevy_ecs::world::World::despawn_no_free",
    "bevy_ecs::world::World::try_despawn",
    "bevy_ecs::world::World::try_despawn_no_free",
];

/// The lints of the bans, and the groups that hold them, named in parts so that this file's own
/// samples hold no attribute of them.
const LINTS: [&str; 2] = [
    concat!("clippy::", "disallowed_methods"),
    concat!("clippy::", "disallowed_types"),
];
const GROUPS: [&str; 2] = [concat!("clippy::", "style"), concat!("clippy::", "all")];

/// Every proxy crate, each the owner of one group of the bans.
pub(crate) const PROXIES: [Proxy; 6] = [
    Proxy {
        dir: "base/store",
        lints: &LINTS,
        methods: &STORAGE_METHODS,
        types: &STORAGE_TYPES,
        dependencies: &[
            "campfire-threads",
            "libc",
            "signal-hook",
            "socket2",
            "tempfile",
            "thiserror",
            "windows",
            "zeroize",
        ],
        names: &["std::fs", "tempfile::"],
        crates: &[],
    },
    Proxy {
        dir: "base/threads",
        lints: &[LINTS[0]],
        methods: &THREAD_METHODS,
        types: &[],
        dependencies: &[],
        names: &[
            "thread::spawn",
            "thread::scope",
            "thread::Builder",
            "spawn_scoped",
            "spawn_unchecked",
        ],
        crates: &[],
    },
    Proxy {
        dir: "base/app-threads",
        lints: &[LINTS[1]],
        methods: &[],
        types: &APP_THREAD_TYPES,
        dependencies: &["bevy_app", "bevy_ecs", "campfire-schedule"],
        names: &["TaskPoolPlugin", "TaskPoolOptions"],
        crates: &[],
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
        crates: &[],
    },
    Proxy {
        dir: "sim/schedule",
        lints: &[LINTS[0]],
        methods: &SCHEDULE_METHODS,
        types: &[],
        dependencies: &["bevy_ecs"],
        names: &["set_build_settings", "configure_schedules", "set_executor"],
        crates: &[],
    },
    Proxy {
        dir: "network/net-despawns",
        lints: &[LINTS[0]],
        methods: &DESPAWN_METHODS,
        types: &[],
        dependencies: &["bevy_ecs", "bevy_replicon"],
        names: &["despawn"],
        crates: &[
            "network/net-despawns",
            "network/net-interpolation",
            "network/net-prediction",
            "network/net-replication",
        ],
    },
];

/// The bans with no proxy, which nothing may exempt: each lets two conflicting sim systems run in
/// either order, runs a query on the compute pool, or seeds a map at random.
const UNOWNED_METHODS: [&str; 12] = [
    "bevy_ecs::schedule::IntoScheduleConfigs::ambiguous_with",
    "bevy_ecs::schedule::Schedule::ignore_ambiguity",
    "bevy_ecs::schedule::Schedules::allow_ambiguous_component",
    "bevy_ecs::world::World::allow_ambiguous_resource",
    "bevy_ecs::system::Query::par_iter",
    "bevy_ecs::system::Query::par_iter_mut",
    "bevy_ecs::system::Query::par_iter_inner",
    "bevy_ecs::system::Query::par_iter_many",
    "bevy_ecs::system::Query::par_iter_many_unique",
    "bevy_ecs::system::Query::par_iter_many_unique_mut",
    "bevy_ecs::query::QueryState::par_iter",
    "bevy_ecs::query::QueryState::par_iter_mut",
];
const UNOWNED_TYPES: [&str; 3] = [
    "std::collections::HashMap",
    "std::collections::HashSet",
    "std::hash::RandomState",
];

/// What names a call of no proxy's group in source, which no proxy's source holds, as its
/// exemption would cover it.
const UNOWNED_NAMES: [&str; 7] = [
    "ambiguous_with",
    "ignore_ambiguity",
    "allow_ambiguous",
    "par_iter",
    "HashMap",
    "HashSet",
    "RandomState",
];

/// The thread-local ban, the one ban an `expect` may exempt outside a proxy: a module whose state
/// an outside API keeps per thread, as a global allocator or tracing's default subscriber does,
/// holds a `thread_local!`, and names no other banned type, which its exemption would cover too.
const THREAD_LOCAL: &str = "std::thread::LocalKey";

/// What names a banned type in source, other than the thread-local one.
const TYPE_NAMES: [&str; 9] = [
    "std::fs",
    "tempfile::",
    "TaskPool",
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
        let own: Vec<String> = proxy
            .crates
            .iter()
            .map(|dir| crate_clippy(dir).expect("a proxy's crate has its own clippy.toml"))
            .collect();
        let bans = if own.is_empty() {
            vec![clippy.clone()]
        } else {
            own
        };
        for bans in &bans {
            let methods = unlisted(bans, "disallowed-methods", proxy.methods);
            assert_eq!(methods, Vec::<&str>::new(), "{}", proxy.dir);
            let types = unlisted(bans, "disallowed-types", proxy.types);
            assert_eq!(types, Vec::<&str>::new(), "{}", proxy.dir);
        }
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
        .chain(&APP_THREAD_TYPES)
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

/// The text of the `clippy.toml` of the crate at `dir`, under `source/crates/`, when it has one.
fn crate_clippy(dir: &str) -> Option<String> {
    let path = source().join("crates").join(dir).join("clippy.toml");
    let bytes = InputFile::read_if_present(&path, CLIPPY_LEN).unwrap()?;
    Some(String::from_utf8(bytes).expect("a clippy.toml is text"))
}

/// The most bytes a `clippy.toml` holds: far past the workspace's.
const CLIPPY_LEN: usize = 1 << 16;

/// `workspace`, the workspace's `clippy.toml`, with the entries of each list that `own`, a
/// crate's, adds to it after that list's own: what a crate's `clippy.toml` holds, as Clippy reads
/// the nearest one alone and so no crate file may drop a ban of the workspace's.
fn with_own_entries(workspace: &str, own: &str) -> String {
    let mut merged = workspace.to_owned();
    for list in ["disallowed-types", "disallowed-methods"] {
        let entries = |text: &str| -> Vec<String> {
            text.split_once(&format!("{list} = [\n"))
                .and_then(|(_, rest)| rest.split_once("\n]"))
                .map_or_else(Vec::new, |(list, _)| {
                    list.lines().map(str::to_owned).collect()
                })
        };
        let shared = entries(workspace);
        let added: Vec<String> = entries(own)
            .into_iter()
            .filter(|line| !shared.contains(line))
            .collect();
        if added.is_empty() {
            continue;
        }
        let head = format!("{list} = [\n");
        let start = merged
            .find(&head)
            .expect("the workspace's clippy.toml holds each list")
            + head.len();
        let end = start + merged[start..].find("\n]").expect("a list ends");
        merged.insert_str(end, &format!("\n{}", added.join("\n")));
    }
    merged
}

#[test]
fn a_crates_clippy_toml_is_the_workspaces_with_its_own_entries() {
    // Each crate's own `clippy.toml` holds the workspace's, written again with `CAMPFIRE_BLESS=1`,
    // and a proxy owns every crate that has one.
    let workspace = read(source().join("clippy.toml").to_str().unwrap());
    let mut owned: Vec<&str> = PROXIES
        .iter()
        .flat_map(|proxy| proxy.crates)
        .copied()
        .collect();
    owned.sort_unstable();
    let mut found = Vec::new();
    for krate in crates() {
        let dir = krate.path.trim_start_matches("crates/");
        let Some(own) = crate_clippy(dir) else {
            continue;
        };
        found.push(krate.path.clone());
        let path = source().join(&krate.path).join("clippy.toml");
        PinnedFile::new(&path).pin(with_own_entries(&workspace, &own).as_bytes(), CLIPPY_LEN);
    }
    found.sort_unstable();
    let owned: Vec<String> = owned.iter().map(|dir| format!("crates/{dir}")).collect();
    assert_eq!(found, owned);
    // An entry the crate adds goes after its list's own; one the workspace holds stays once.
    let workspace = "a = 1\ndisallowed-methods = [\n    { path = \"x\" },\n]\n";
    let own = "a = 1\ndisallowed-methods = [\n    { path = \"x\" },\n    { path = \"y\" },\n]\n";
    assert_eq!(with_own_entries(workspace, own), own);
    assert_eq!(with_own_entries(workspace, workspace), workspace);
    let stale = "a = 2\ndisallowed-methods = [\n    { path = \"y\" },\n]\n";
    assert_eq!(with_own_entries(workspace, stale), own);
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
        ("crates/sim/schedule/src/lib.rs", inner(types), vec![1]),
        ("crates/sim/schedule/src/lib.rs", inner(methods), vec![]),
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

/// The line of a crate's root that bans floats from the sim, as clippy's `float_arithmetic` is
/// in no group the workspace turns on (07-determinism-core.md, One step: an `i128`, rounded once).
const FLOAT_BAN: &str = "#![warn(clippy::float_arithmetic)]";

#[test]
fn each_crate_a_match_runs_on_bans_floats() {
    // A proc-macro crate's code runs in the compiler, not in a match.
    let reached = sim_closure(&manifests());
    let mut checked = 0;
    let mut missing = Vec::new();
    for krate in crates()
        .iter()
        .filter(|krate| reached.contains(&krate.name))
    {
        checked += 1;
        if krate.manifest.is_proc_macro() {
            continue;
        }
        let root = source().join(&krate.path).join("src/lib.rs");
        let text = read(root.to_str().unwrap());
        if !text.lines().any(|line| line == FLOAT_BAN) {
            missing.push(krate.name.clone());
        }
    }
    assert_eq!(missing, Vec::<String>::new());
    assert_eq!(checked, reached.len());
}
