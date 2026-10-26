use std::fmt::Write as _;

use proc_macro2::TokenStream;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{AttrStyle, Attribute, Meta, Path, Token};

use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;

use crate::allowlist::Allowlist;
use crate::named_path::NamedPath;
use crate::source_file::SourceFile;
use crate::workspace::{Workspace, read, sim_closure, source};

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
    /// the nearest `clippy.toml` alone, with the comment and the reason of its entries; none when
    /// the workspace's bans it everywhere.
    pub(crate) crates: Option<CrateBans>,
}

/// The crates whose own `clippy.toml` holds a proxy's bans, each the workspace's with the bans
/// added under `comment`, each entry of `reason`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CrateBans {
    pub(crate) dirs: &'static [&'static str],
    pub(crate) comment: &'static str,
    pub(crate) reason: &'static str,
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

/// The calls that start Bevy's task pools, which only the plugin of `app-threads` makes.
const APP_THREAD_METHODS: [&str; 4] = [
    "bevy_tasks::TaskPool::new",
    "bevy_tasks::ComputeTaskPool::get_or_init",
    "bevy_tasks::AsyncComputeTaskPool::get_or_init",
    "bevy_tasks::IoTaskPool::get_or_init",
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
const CODEC_METHODS: [&str; 35] = [
    "postcard::to_allocvec",
    "postcard::to_allocvec_cobs",
    "postcard::to_stdvec",
    "postcard::to_stdvec_cobs",
    "postcard::to_extend",
    "postcard::to_io",
    "postcard::to_slice",
    "postcard::to_slice_cobs",
    "postcard::serialize_with_flavor",
    "postcard::from_bytes",
    "postcard::from_bytes_cobs",
    "postcard::take_from_bytes",
    "postcard::take_from_bytes_cobs",
    "postcard::from_io",
    "toml::from_str",
    "toml::from_slice",
    "toml::to_string",
    "toml::to_string_pretty",
    "serde_json::from_str",
    "serde_json::from_slice",
    "serde_json::from_reader",
    "serde_json::to_string",
    "serde_json::to_string_pretty",
    "serde_json::to_vec",
    "serde_json::to_vec_pretty",
    "serde_json::to_writer",
    "serde_json::to_writer_pretty",
    "serde_json::from_value",
    "serde_json::to_value",
    "toml::Value::try_into",
    "toml::map::Map::try_into",
    "bevy_replicon::postcard_utils::to_extend_mut",
    "bevy_replicon::postcard_utils::from_buf",
    "bevy_replicon::postcard_utils::entity_to_extend_mut",
    "bevy_replicon::postcard_utils::entity_from_buf",
];

/// The types only `codec` holds, each banned whole so every method of it is: a value type among
/// them, which is a deserializer of its own.
const CODEC_TYPES: [&str; 11] = [
    "postcard::Serializer",
    "postcard::Deserializer",
    "toml::Serializer",
    "toml::Deserializer",
    "serde_json::Serializer",
    "serde_json::Deserializer",
    "serde_json::Value",
    "toml::Value",
    "toml::Table",
    "png::Encoder",
    "png::Decoder",
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
/// (11-net-stack/replication.md, The server alone ends a replicated entity's life on the
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

/// The lints of the bans, and the groups that hold them.
const LINTS: [&str; 2] = ["clippy::disallowed_methods", "clippy::disallowed_types"];
const GROUPS: [&str; 2] = ["clippy::style", "clippy::all"];

/// Every proxy crate, each the owner of one group of the bans.
pub(crate) const PROXIES: [Proxy; 7] = [
    Proxy {
        dir: "base/store",
        lints: &LINTS,
        methods: &STORAGE_METHODS,
        types: &STORAGE_TYPES,
        dependencies: &[
            "campfire-platform",
            "campfire-threads",
            "tempfile",
            "thiserror",
            "zeroize",
        ],
        names: &["std::fs", "tempfile::"],
        crates: None,
    },
    // The OS's promises under the files, which open, name and expose files as `store` does.
    Proxy {
        dir: "base/platform",
        lints: &LINTS,
        methods: &STORAGE_METHODS,
        types: &STORAGE_TYPES,
        dependencies: &[
            "campfire-common",
            "libc",
            "signal-hook",
            "socket2",
            "windows",
        ],
        names: &[
            "std::fs",
            "libc::",
            "signal_hook::",
            "socket2::",
            "windows::",
        ],
        crates: None,
    },
    Proxy {
        dir: "base/threads",
        lints: &[LINTS[0]],
        methods: &THREAD_METHODS,
        types: &[],
        // Its jobs' names, a `NamedEnum` of `common`.
        dependencies: &["campfire-common"],
        names: &[
            "thread::spawn",
            "thread::scope",
            "thread::Builder",
            "spawn_scoped",
            "spawn_unchecked",
        ],
        crates: None,
    },
    Proxy {
        dir: "base/app-threads",
        lints: &[LINTS[1]],
        methods: &APP_THREAD_METHODS,
        types: &APP_THREAD_TYPES,
        dependencies: &["bevy_app", "bevy_ecs", "bevy_tasks", "campfire-schedule"],
        names: &["TaskPoolPlugin", "TaskPoolOptions"],
        crates: None,
    },
    Proxy {
        dir: "base/codec",
        lints: &LINTS,
        methods: &CODEC_METHODS,
        types: &CODEC_TYPES,
        dependencies: &[
            "arrayvec",
            "png",
            "postcard",
            "serde",
            "serde_json",
            "thiserror",
            "toml",
        ],
        names: &[
            "postcard::",
            "serde_json::",
            "toml::",
            "png::",
            "postcard_utils",
        ],
        crates: None,
    },
    Proxy {
        dir: "sim/schedule",
        lints: &[LINTS[0]],
        methods: &SCHEDULE_METHODS,
        types: &[],
        dependencies: &["bevy_ecs"],
        names: &["set_build_settings", "configure_schedules", "set_executor"],
        crates: None,
    },
    Proxy {
        dir: "network/net-despawns",
        lints: &[LINTS[0]],
        methods: &DESPAWN_METHODS,
        types: &[],
        dependencies: &["bevy_ecs", "bevy_replicon"],
        names: &["despawn"],
        crates: Some(CrateBans {
            dirs: &[
                "network/net-despawns",
                "network/net-interpolation",
                "network/net-prediction",
                "network/net-replication",
            ],
            comment: "campfire-net-despawns alone despawns in the network stack's replication crates (11-net-stack/replication.md, The server alone ends a replicated entity's life on the receiver): Bevy's despawn takes each child the server still owns.",
            reason: "despawn through campfire_net_despawns::DespawnReplicated",
        }),
    },
];

/// The bans with no proxy, which nothing may exempt: each lets two conflicting sim systems run in
/// either order, runs a system's work on the compute pool, seeds a map at random, draws the OS's
/// random bytes past the one source, or names the OS's temporary directory.
const UNOWNED_METHODS: [&str; 17] = [
    "std::env::temp_dir",
    "bevy_ecs::schedule::IntoScheduleConfigs::ambiguous_with",
    "bevy_ecs::schedule::IntoScheduleConfigs::ambiguous_with_all",
    "bevy_ecs::schedule::Schedule::ignore_ambiguity",
    "bevy_ecs::schedule::Schedules::ignore_ambiguity",
    "bevy_ecs::schedule::Schedules::allow_ambiguous_component",
    "bevy_ecs::schedule::Schedules::allow_ambiguous_resource",
    "bevy_ecs::world::World::allow_ambiguous_component",
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
const UNOWNED_TYPES: [&str; 6] = [
    "ring::rand::SystemRandom",
    "std::collections::HashMap",
    "std::collections::HashSet",
    "std::hash::RandomState",
    "bevy_ecs::system::ParallelCommands",
    "bevy_utils::Parallel",
];

/// What names a call of no proxy's group in source, which no proxy's source holds, as its
/// exemption would cover it.
const UNOWNED_NAMES: [&str; 10] = [
    "SystemRandom",
    "temp_dir",
    "ambiguous_with",
    "ignore_ambiguity",
    "allow_ambiguous",
    "par_iter",
    "ParallelCommands",
    "HashMap",
    "HashSet",
    "RandomState",
];

/// The thread-local ban, the one ban an `expect` may exempt outside a proxy: a module whose state
/// an outside API keeps per thread, as a global allocator or tracing's default subscriber does,
/// holds a `thread_local!`, and names no other banned type, which its exemption would cover too.
const THREAD_LOCAL: &str = "std::thread::LocalKey";

/// What names a banned type in source, other than the thread-local one.
const TYPE_NAMES: [&str; 13] = [
    "SystemRandom",
    "std::fs",
    "tempfile::",
    "TaskPool",
    "postcard::",
    "toml::",
    "serde_json::",
    "png::",
    "HashMap",
    "HashSet",
    "RandomState",
    "ParallelCommands",
    "bevy_utils::Parallel",
];

/// The text of the list `list`, `disallowed-methods` or `disallowed-types`, of `clippy`, a
/// `clippy.toml`.
fn list_text<'a>(clippy: &'a str, list: &str) -> &'a str {
    clippy
        .split_once(&format!("{list} = ["))
        .and_then(|(_, rest)| rest.split_once("\n]"))
        .map_or("", |(list, _)| list)
}

/// The paths of `rules` that the list `list` of `clippy` does not name.
fn unlisted<'a>(clippy: &str, list: &str, rules: &[&'a str]) -> Vec<&'a str> {
    let list = list_text(clippy, list);
    let named = |path: &&str| list.contains(&format!("path = \"{path}\""));
    rules.iter().copied().filter(|path| !named(path)).collect()
}

/// The paths the list `list` of `clippy` names that `rules` does not hold, so a ban added to the
/// file and to no group fails as one deleted does.
fn ungrouped<'a>(clippy: &'a str, list: &str, rules: &[&str]) -> Vec<&'a str> {
    let paths = list_text(clippy, list)
        .split("path = \"")
        .skip(1)
        .filter_map(|entry| entry.split_once('"').map(|(path, _)| path));
    paths.filter(|path| !rules.contains(path)).collect()
}

/// The lines of `text`, the source at `path` under `source/`, whose attribute exempts code from
/// either lint other than as the proxies let it: only an `expect`, at a proxy's crate root, of a
/// lint that proxy exempts, or of the types' lint at the head of a module that holds a
/// `thread_local!` and names no other banned type, the proxy's under a `cfg_attr` too, as the
/// platform's calls differ by OS; never `allow`, a group that holds the lint, or an attribute on
/// an item or a statement, so a stale one fails where it applies and none hides.
fn misplaced_exemptions(path: &str, text: &str) -> Vec<usize> {
    struct Exemptions<'a> {
        path: &'a str,
        thread_module: bool,
        found: Vec<usize>,
    }
    impl Exemptions<'_> {
        /// Takes the lints of `level` an attribute names, `inner` at its module's head, `gated`
        /// under a `cfg_attr`.
        fn take(&mut self, level: &Path, lints: &[Path], inner: bool, gated: bool) {
            let level = path_text(level);
            if ["warn", "deny", "forbid"].contains(&level.as_str()) {
                return;
            }
            for lint in lints {
                let name = path_text(lint);
                let Some(&name) = LINTS.iter().chain(&GROUPS).find(|banned| **banned == name)
                else {
                    continue;
                };
                let proxied = PROXIES
                    .iter()
                    .any(|proxy| proxy.root() == self.path && proxy.lints.contains(&name));
                let thread_state = name == LINTS[1] && self.thread_module;
                let allowed = inner && level == "expect" && (proxied || thread_state && !gated);
                if !allowed {
                    let at = lint.segments.first().expect("a lint has a name");
                    self.found.push(at.ident.span().start().line);
                }
            }
        }
    }
    impl<'ast> Visit<'ast> for Exemptions<'_> {
        fn visit_attribute(&mut self, attribute: &'ast Attribute) {
            let inner = matches!(attribute.style, AttrStyle::Inner(_));
            if attribute.path().is_ident("cfg_attr") {
                let Ok(metas) =
                    attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return;
                };
                for meta in metas.iter().skip(1) {
                    if let Meta::List(list) = meta {
                        let lints = lint_paths(&list.tokens);
                        self.take(&list.path, &lints, inner, true);
                    }
                }
            } else if let Meta::List(list) = &attribute.meta {
                let lints = lint_paths(&list.tokens);
                self.take(&list.path, &lints, inner, false);
            }
        }
    }
    // Each lint and group it reads is clippy's, so a file that names no lint of clippy's holds
    // none, and its parse is skipped.
    if !text.contains("clippy") {
        return Vec::new();
    }
    let paths = NamedPath::of(text);
    let thread_module = NamedPath::holds(&paths, "thread_local")
        && !TYPE_NAMES
            .iter()
            .any(|banned| NamedPath::holds(&paths, banned));
    let file = syn::parse_file(text).expect("a workspace source parses");
    let mut exemptions = Exemptions {
        path,
        thread_module,
        found: Vec::new(),
    };
    exemptions.visit_file(&file);
    exemptions.found.sort_unstable();
    exemptions.found
}

/// A path as its segments joined by `::`.
fn path_text(path: &Path) -> String {
    let names: Vec<String> = (path.segments.iter())
        .map(|segment| segment.ident.to_string())
        .collect();
    names.join("::")
}

/// The paths a lint level's list names, its `reason` left out.
fn lint_paths(tokens: &TokenStream) -> Vec<Path> {
    let metas = Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(tokens.clone())
        .unwrap_or_default();
    metas
        .into_iter()
        .filter_map(|meta| match meta {
            Meta::Path(path) => Some(path),
            Meta::List(_) | Meta::NameValue(_) => None,
        })
        .collect()
}

/// What of another group's names, another proxy's that `proxy` does not list too, or no proxy's,
/// `text`, a source of `proxy`'s, holds.
pub(crate) fn foreign_names(proxy: &Proxy, text: &str) -> Vec<&'static str> {
    let paths = NamedPath::of(text);
    let mut found: Vec<&'static str> = Vec::new();
    let names = PROXIES
        .iter()
        .filter(|other| other.dir != proxy.dir)
        .flat_map(|other| other.names.iter())
        .filter(|name| !proxy.names.contains(name))
        .chain(&UNOWNED_NAMES)
        .copied();
    for name in names {
        if NamedPath::holds(&paths, name) && !found.contains(&name) {
            found.push(name);
        }
    }
    found
}

pub(crate) fn each_banned_call_goes_through_its_proxy_alone(workspace: &Workspace) {
    let clippy = read(source().join("clippy.toml").to_str().unwrap());
    let sources = workspace.sources();
    for proxy in &PROXIES {
        let own: Vec<String> = (proxy.crates.iter())
            .flat_map(|crates| crates.dirs)
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
    let grouped = |list: fn(&Proxy) -> &'static [&'static str], unowned: &[&'static str]| {
        let proxied = PROXIES.iter().filter(|proxy| proxy.crates.is_none());
        let mut rules: Vec<&str> = proxied.flat_map(list).copied().collect();
        rules.extend(unowned);
        rules
    };
    let methods = grouped(|proxy| proxy.methods, &UNOWNED_METHODS);
    let types = grouped(
        |proxy| proxy.types,
        &[&UNOWNED_TYPES[..], &[THREAD_LOCAL]].concat(),
    );
    assert_eq!(
        ungrouped(&clippy, "disallowed-methods", &methods),
        Vec::<&str>::new()
    );
    assert_eq!(
        ungrouped(&clippy, "disallowed-types", &types),
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

/// The `clippy.toml` of a crate that `bans`, of `proxy`, names: `workspace`'s, with the proxy's
/// methods and types added at the end of their lists, under the bans' comment, each of their
/// reason.
fn with_proxy_bans(workspace: &str, proxy: &Proxy, bans: &CrateBans) -> String {
    let mut merged = workspace.to_owned();
    for (list, paths) in [
        ("disallowed-types", proxy.types),
        ("disallowed-methods", proxy.methods),
    ] {
        if paths.is_empty() {
            continue;
        }
        let mut added = format!("\n    # {}", bans.comment);
        for path in paths {
            write!(
                added,
                "\n    {{ path = \"{path}\", reason = \"{}\" }},",
                bans.reason
            )
            .expect("a String takes each write");
        }
        let head = format!("{list} = [\n");
        let start = merged
            .find(&head)
            .expect("the workspace's clippy.toml holds each list")
            + head.len();
        let end = start + merged[start..].find("\n]").expect("a list ends");
        merged.insert_str(end, &added);
    }
    merged
}

pub(crate) fn a_crates_clippy_toml_is_the_workspaces_with_its_own_entries(workspace: &Workspace) {
    // Each crate's own `clippy.toml` is the workspace's with its proxy's bans, written again with
    // `CAMPFIRE_BLESS=1`, and a proxy owns every crate that has one.
    let shared = read(source().join("clippy.toml").to_str().unwrap());
    let mut owned = Vec::new();
    for proxy in &PROXIES {
        let Some(bans) = &proxy.crates else {
            continue;
        };
        let text = with_proxy_bans(&shared, proxy, bans);
        for dir in bans.dirs {
            let path = source().join("crates").join(dir).join("clippy.toml");
            PinnedFile::new(&path).pin(text.as_bytes(), CLIPPY_LEN);
            owned.push(format!("crates/{dir}"));
        }
    }
    owned.sort_unstable();
    let mut found: Vec<String> = workspace
        .crates()
        .iter()
        .filter(|krate| crate_clippy(krate.path.trim_start_matches("crates/")).is_some())
        .map(|krate| krate.path.clone())
        .collect();
    found.sort_unstable();
    assert_eq!(found, owned);
    // The bans go at the end of their list, under their comment; a list the proxy bans nothing of
    // stays the workspace's.
    let workspace =
        "a = 1\ndisallowed-types = [\n]\ndisallowed-methods = [\n    { path = \"x\" },\n]\n";
    let proxy = Proxy {
        methods: &["y", "z"],
        ..PROXIES[0]
    };
    let bans = CrateBans {
        dirs: &[],
        comment: "c",
        reason: "r",
    };
    let proxy = Proxy {
        types: &[],
        ..proxy
    };
    assert_eq!(
        with_proxy_bans(workspace, &proxy, &bans),
        "a = 1\ndisallowed-types = [\n]\ndisallowed-methods = [\n    { path = \"x\" },\n    # c\n    { path = \"y\", reason = \"r\" },\n    { path = \"z\", reason = \"r\" },\n]\n"
    );
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
            format!("fn f() {{\n    {}    let x = 1;\n}}\n", expect(types)),
            vec![2],
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
            format!("fn f() {{\n    {}    let x = 1;\n}}\n", expect(methods)),
            vec![2],
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
        (
            "crates/x/src/a.rs",
            format!(
                "{}use std::{{\n    collections::HashMap,\n}};\nthread_local! {{}}\n",
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
        // A proxy's `expect` under a condition, as the platform's is under its OS's; an `allow`
        // so, or an `expect` so elsewhere, found.
        (
            "crates/base/platform/src/lib.rs",
            format!("#![cfg_attr(feature = \"os\", expect({methods}, reason = \"r\"))]\n"),
            vec![],
        ),
        (
            "crates/base/platform/src/lib.rs",
            format!("#![cfg_attr(feature = \"os\", allow({methods}))]\n"),
            vec![1],
        ),
        (
            "crates/x/src/lib.rs",
            format!("#![cfg_attr(feature = \"os\", expect({methods}, reason = \"r\"))]\n"),
            vec![1],
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

/// The line of each place where the production code of `file`, not a comment nor a gated item,
/// names Bevy's `RemovedComponents`.
fn app_removals(file: &SourceFile) -> Vec<usize> {
    let Some(code) = file.production() else {
        return Vec::new();
    };
    (1..)
        .zip(code.lines())
        .filter(|(_, line)| line.contains("RemovedComponents"))
        .map(|(at, _)| at)
        .collect()
}

pub(crate) fn no_tick_reads_removals_that_age_with_the_app(workspace: &Workspace) {
    let found: Vec<String> = workspace
        .sources()
        .iter()
        .filter(|file| file.gate.is_none())
        .filter(|file| TICK_CRATES.iter().any(|dir| file.path.starts_with(dir)))
        .flat_map(|file| {
            app_removals(file)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_tick_s_removal_reader_is_found_in_production_code_alone() {
    // A reader in code is found; a mention in a comment, and a test's reader, are not.
    let sample = concat!(
        "/// Unlike `RemovedComponents`.\n",
        "fn f(gone: RemovedComponents<'_, '_, A>) {} // here\n",
        "\n#[cfg(test)]\nmod tests {\n",
        "    fn g(gone: RemovedComponents<'_, '_, A>) {}\n",
        "}\n",
    );
    assert_eq!(
        app_removals(&SourceFile::new("crates/sim/a/src/a.rs", sample)),
        [2]
    );
}

/// The line of a crate's root that bans floats from the sim, as clippy's `float_arithmetic` is
/// in no group the workspace turns on (07-determinism-core.md, One step: an `i128`, rounded once).
const FLOAT_BAN: &str = "#![warn(clippy::float_arithmetic)]";

pub(crate) fn each_crate_a_match_runs_on_bans_floats(workspace: &Workspace) {
    // A proc-macro crate's code runs in the compiler, not in a match.
    let reached = sim_closure(workspace.crates());
    let mut checked = 0;
    let mut missing = Vec::new();
    for krate in workspace
        .crates()
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

/// The calls, the constant and Bevy's clocks of `Time<_>` that read the wall clock, which no crate
/// a match runs on names (02-engine-core.md, Determinism rules): a match runs the same on a server,
/// a client and a verifier only on its ticks.
const WALL_CLOCK: [&str; 5] = [
    "Instant::now",
    "SystemTime::now",
    "UNIX_EPOCH",
    "Real",
    "Virtual",
];

/// The one module of a match's crates that reads the wall clock: the stage probes, which time each
/// stage of a real tick from outside the state (02-engine-core.md, Stage probes keep the hashes).
const WALL_CLOCK_READERS: [&str; 1] = ["crates/sim/sim/src/sim_update/stage_clock/"];

/// The wall clock reads `code` names, of `WALL_CLOCK`, each a path or a path's last segments.
fn wall_clock_reads(code: &str) -> Vec<&'static str> {
    if !WALL_CLOCK.iter().any(|read| code.contains(read)) {
        return Vec::new();
    }
    let paths = NamedPath::of(code);
    let names = |read: &&&str| {
        let tail = format!("::{read}");
        paths
            .iter()
            .any(|path| path.text == **read || path.text.ends_with(&tail))
    };
    WALL_CLOCK.iter().filter(names).copied().collect()
}

pub(crate) fn no_crate_a_match_runs_on_reads_the_wall_clock(workspace: &Workspace) {
    let reached = sim_closure(workspace.crates());
    let readers = Allowlist::new(&WALL_CLOCK_READERS);
    let mut found = Vec::new();
    for krate in (workspace.crates().iter()).filter(|krate| reached.contains(&krate.name)) {
        // A bench times its own runs, which no match reads.
        let measured = |path: &str| path.ends_with("/bench.rs") || path.contains("/bench/");
        for file in workspace
            .sources_of(krate)
            .filter(|file| !measured(&file.path))
        {
            let Some(code) = file.library() else {
                continue;
            };
            let reads = wall_clock_reads(code);
            if !reads.is_empty() && !readers.allows(|dir| file.path.starts_with(dir)) {
                found.push(format!("{}: {}", file.path, reads.join(", ")));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert_eq!(readers.unused(), Vec::<&&str>::new());
}

#[test]
fn a_wall_clock_read_is_found_by_the_paths_code_names() {
    // A read of the OS's time, by a call or by Bevy's real time, is found; a clock of the match's
    // own is not, nor a string's mention.
    assert_eq!(
        wall_clock_reads("let t = Instant::now(); let r: Res<Time<Real>>;"),
        ["Instant::now", "Real"]
    );
    assert_eq!(
        wall_clock_reads("let t = std::time::SystemTime::now();"),
        ["SystemTime::now"]
    );
    assert!(wall_clock_reads("let t = clock.now(); let f: Res<Time<Fixed>>;").is_empty());
    assert!(wall_clock_reads("let s = \"Instant::now\";").is_empty());
}
