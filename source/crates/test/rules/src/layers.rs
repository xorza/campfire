//! The layer test: each crate requires only crates of its own layer or one below
//! (02-engine-core.md, Structural rules).

use std::collections::BTreeMap;

use crate::allowlist::Allowlist;
use crate::workspace::{Crate, Workspace};

/// The dependencies on a higher layer that hold for a reason, each by its crate's name and the
/// crate it requires: `app-threads` sets the executor of an app's schedules through `schedule`,
/// the proxy of schedule settings, which depends on no crate and sits with the sim it serves
/// (02-engine-core.md, A directory names a domain, and the manifest the kind).
const UPWARD: [(&str, &str); 1] = [("campfire-app-threads", "campfire-schedule")];

/// The layers of the workspace's crates by their directory under `source/`, lowest first: the
/// base crates, the sim's, the contracts below the capabilities, the capabilities, the network
/// stack's `net-<name>` crates, the session's, `net`, the session over the stack, and the apps,
/// tests and checks above all. The first prefix of a crate's path that a layer names is its.
const LAYERS: [&[&str]; 8] = [
    &["crates/base/"],
    &["crates/sim/"],
    &["crates/capabilities/core/"],
    &["crates/capabilities/"],
    &["crates/network/net-"],
    &["crates/session/"],
    &["crates/network/net"],
    &["crates/apps/", "crates/test/", "checks/"],
];

/// The layer of the crate at `path`; none for one no layer names.
fn layer(path: &str) -> Option<usize> {
    let within = format!("{path}/");
    LAYERS
        .iter()
        .position(|prefixes| prefixes.iter().any(|prefix| within.starts_with(prefix)))
}

/// Each required dependency of `crates` on a crate of a higher layer but those `allowed` holds,
/// and each crate of no layer.
fn upward(crates: &[Crate], allowed: &Allowlist<(&str, &str)>) -> Vec<String> {
    let layers: BTreeMap<&str, Option<usize>> = crates
        .iter()
        .map(|krate| (krate.name.as_str(), layer(&krate.path)))
        .collect();
    let mut found = Vec::new();
    for krate in crates {
        let Some(own) = layers[krate.name.as_str()] else {
            found.push(format!("{} lies in no layer", krate.path));
            continue;
        };
        for other in krate.manifest.required() {
            if let Some(&Some(theirs)) = layers.get(other)
                && theirs > own
                && !allowed.allows(|&(from, to)| from == krate.name && to == other)
            {
                found.push(format!(
                    "{} requires {other}, of a higher layer",
                    krate.name
                ));
            }
        }
    }
    found
}

pub(crate) fn each_crate_requires_only_its_own_layer_or_lower(workspace: &Workspace) {
    let allowed = Allowlist::new(&UPWARD);
    assert_eq!(upward(workspace.crates(), &allowed), Vec::<String>::new());
    assert_eq!(allowed.unused(), Vec::<&(&str, &str)>::new());
}

#[test]
fn the_layer_rule_fails_on_what_breaks_it() {
    // A base crate that requires a sim crate breaks it; a sim crate that requires a base one,
    // and one that requires a test crate optionally, as its benches do, keep it; `net` takes a
    // session crate, which a stack crate may not.
    let sample = |path: &str, name: &str, dependencies: &str| {
        let mut krate = Crate::sample(
            name,
            &format!("[package]\nname = \"{name}\"\n\n[dependencies]\n{dependencies}"),
        );
        krate.path = path.to_owned();
        krate
    };
    let crates = [
        sample("crates/base/common", "common", "math.workspace = true\n"),
        sample(
            "crates/sim/math",
            "math",
            "common.workspace = true\nmeasure = { workspace = true, optional = true }\n",
        ),
        sample("crates/test/measure", "measure", ""),
        sample("crates/session/host", "host", "net-core.workspace = true\n"),
        sample(
            "crates/network/net-core",
            "net-core",
            "host.workspace = true\n",
        ),
        sample("crates/network/net", "net", "host.workspace = true\n"),
        sample("crates/other/x", "x", ""),
    ];
    assert_eq!(
        upward(&crates, &Allowlist::new(&[])),
        [
            "common requires math, of a higher layer",
            "net-core requires host, of a higher layer",
            "crates/other/x lies in no layer",
        ]
    );
}
