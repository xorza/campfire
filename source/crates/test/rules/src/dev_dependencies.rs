use std::collections::BTreeMap;

use crate::manifest::CrateKind;
use crate::workspace::{Crate, DevEdges, Workspace, test_build};

/// The tools made for tests alone that any crate's tests may take (02-engine-core.md, A
/// dev-dependency adds only test support to its crate's build).
const TEST_TOOLS: [&str; 1] = ["proptest"];

/// The base crates whose `internals` gives any crate's tests its support: `codec` a type's
/// formats, `log` the check of the warnings, `store` test directories and pinned files, and
/// `threads` parallel runs (02-engine-core.md, A dev-dependency adds only test support to its
/// crate's build).
const TEST_SUPPORT: [&str; 4] = [
    "campfire-codec",
    "campfire-log",
    "campfire-store",
    "campfire-threads",
];

/// A crate that re-exports others, which no test takes, as no caller depends on a facade
/// (02-engine-core.md, One published surface).
const FACADES: [&str; 1] = ["bevy"];

/// Each crate outside the workspace that a crate's test build holds through another such crate
/// alone, which no manifest here names: the crate whose tests name it, the crate, and the one of
/// its build that brings it.
const PAST_THE_WORKSPACE: [[&str; 3]; 1] = [
    // The shader validator the renderer runs, which a test runs on the client's shaders.
    ["campfire-client", "naga", "wgpu"],
];

/// What breaks the rule of a dev-dependency, by crate: one that is neither a dependency of the
/// crate nor the crate itself adds to the crate's test build a test crate, a tool of
/// `TEST_TOOLS`, a crate of `TEST_SUPPORT`, or a crate the build holds already, never a facade; a
/// crate of kind `test` ships nothing, so its dev-dependencies keep only the rule that none builds
/// on it, as its tests' build would hold a second copy of it.
fn dev_dependency_problems(manifests: &[Crate]) -> Vec<String> {
    let kinds: BTreeMap<&str, Option<CrateKind>> = manifests
        .iter()
        .map(|krate| (krate.name.as_str(), krate.manifest.kind()))
        .collect();
    let mut problems = Vec::new();
    for Crate { name, manifest, .. } in manifests {
        let mut second_copies = Vec::new();
        for (other, _) in manifest.dev_dependencies() {
            let builds_on = kinds.contains_key(other)
                && other != name
                && test_build(manifests, other, DevEdges::Left).contains(name.as_str());
            if builds_on {
                problems.push(format!(
                    "{name} dev-depends on {other}, which builds on it: a second copy"
                ));
                second_copies.push(other);
            }
        }
        if kinds[name.as_str()] == Some(CrateKind::Test) {
            continue;
        }
        let build = test_build(manifests, name, DevEdges::Left);
        let past = |other: &str| {
            PAST_THE_WORKSPACE
                .iter()
                .any(|[taker, past, _]| *taker == name && *past == other)
        };
        for (other, _) in manifest.dev_dependencies() {
            let normal = manifest.dependencies().any(|(normal, _)| normal == other);
            if other == name || normal || second_copies.contains(&other) {
                continue;
            }
            if FACADES.contains(&other) {
                problems.push(format!("{name} dev-depends on the facade {other}"));
                continue;
            }
            let allowed =
                is_test_support(manifests, name, other) || build.contains(other) || past(other);
            if !allowed {
                problems.push(format!(
                    "{name} dev-depends on {other}, past its tests' build"
                ));
            }
        }
    }
    problems
}

/// Whether the tests of `taker`, a crate of `manifests`, may take `other` as test support
/// beside what their build holds: a tool of `TEST_TOOLS`, or a test crate or a crate of
/// `TEST_SUPPORT` that does not build on `taker`.
pub(crate) fn is_test_support(manifests: &[Crate], taker: &str, other: &str) -> bool {
    let test_crate = manifests
        .iter()
        .any(|krate| krate.name == other && krate.manifest.kind() == Some(CrateKind::Test));
    let builds_on = || {
        let workspace = manifests.iter().any(|krate| krate.name == other);
        workspace && test_build(manifests, other, DevEdges::Left).contains(taker)
    };
    let support = test_crate || TEST_SUPPORT.contains(&other);
    TEST_TOOLS.contains(&other) || (support && !builds_on())
}

/// The entries of `PAST_THE_WORKSPACE` that no longer name a dev-dependency past its crate's
/// test build.
fn stale_past_entries(manifests: &[Crate]) -> Vec<String> {
    PAST_THE_WORKSPACE
        .iter()
        .filter(|[taker, past, _]| {
            let Some(Crate { manifest, .. }) = manifests.iter().find(|krate| krate.name == *taker)
            else {
                return true;
            };
            let takes = manifest.dev_dependencies().any(|(other, _)| other == *past);
            !takes || test_build(manifests, taker, DevEdges::Left).contains(past)
        })
        .map(|[taker, past, _]| format!("{taker} {past}"))
        .collect()
}

pub(crate) fn a_dev_dependency_adds_only_test_support(workspace: &Workspace) {
    let manifests = workspace.crates();
    assert_eq!(dev_dependency_problems(manifests), Vec::<String>::new());
    assert_eq!(stale_past_entries(manifests), Vec::<String>::new());
    // `a` dev-depends on itself, on `b`, which its build holds through `c` with `c`'s `internals`,
    // on `e`, which `b`'s `internals` turns on through `c`'s weak entry, written before the entry
    // that turns `b` on, on `t`, a test crate, on `campfire-store`, `proptest` and `bevy`, and on
    // `d`, which its build lacks. `t` builds on `a`, so `a`'s tests would hold a second copy of
    // `a`; `t` is a test crate, so its own dev-dependency on `d` breaks no rule.
    let crates = [
        (
            "a",
            concat!(
                "[package]\nname = \"a\"\n\n[package.metadata.campfire]\nkind = \"glue\"\n\n",
                "[features]\ninternals = [\"c/internals\"]\n\n[dependencies]\nc.workspace = true\n\n",
                "[dev-dependencies]\na = { workspace = true, features = [\"internals\"] }\n",
                "b.workspace = true\nbevy.workspace = true\ncampfire-store.workspace = true\n",
                "d.workspace = true\ne.workspace = true\nproptest.workspace = true\n",
                "t.workspace = true\n",
            ),
        ),
        (
            "b",
            concat!(
                "[package.metadata.campfire]\nkind = \"glue\"\n\n[features]\ninternals = [\"dep:e\"]\n\n",
                "[dependencies]\ne = { workspace = true, optional = true }\n",
            ),
        ),
        (
            "c",
            concat!(
                "[package.metadata.campfire]\nkind = \"glue\"\n\n[features]\n",
                "internals = [\"b?/internals\", \"dep:b\"]\n\n",
                "[dependencies]\nb = { workspace = true, optional = true }\n",
            ),
        ),
        ("d", "[package.metadata.campfire]\nkind = \"glue\"\n"),
        ("e", "[package.metadata.campfire]\nkind = \"glue\"\n"),
        (
            "t",
            concat!(
                "[package.metadata.campfire]\nkind = \"test\"\n\n[dependencies]\na.workspace = true\n\n",
                "[dev-dependencies]\nd.workspace = true\n",
            ),
        ),
    ]
    .map(|(name, manifest)| Crate::sample(name, manifest));
    assert_eq!(
        dev_dependency_problems(&crates),
        [
            "a dev-depends on t, which builds on it: a second copy",
            "a dev-depends on the facade bevy",
            "a dev-depends on d, past its tests' build",
        ]
    );
}
