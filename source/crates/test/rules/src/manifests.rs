use std::collections::{BTreeMap, BTreeSet};
use std::panic;

use campfire_store::internals::{CargoManifest, SourceFile};

use crate::bans::{PROXIES, foreign_names};
use crate::workspace::{
    Crate, crate_dependencies, crates, holder, manifests, read, required_dependencies, sim_closure,
    source, sources,
};

#[test]
fn common_and_each_proxy_depend_on_their_listed_crates_alone() {
    // `common` holds plain values, so it needs serde and its derives, its own derives' crate,
    // the two hashes that make its 32-byte values, SHA-256 and BLAKE3, `zeroize`, which erases
    // a secret as it drops, and `backtrace` for the allocation audit, which only a test build
    // turns on; a proxy reaches only its own group's crates, so its exemption covers no call of
    // another group.
    assert_eq!(
        crate_dependencies("base/common"),
        [
            "backtrace",
            "blake3",
            "campfire-common-derive",
            "derive_more",
            "serde",
            "sha2",
            "zeroize"
        ]
    );
    let manifest = read(
        source()
            .join("crates/base/common/Cargo.toml")
            .to_str()
            .unwrap(),
    );
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

/// The crate of `common`'s derives, which `common` alone names and re-exports, so a caller takes
/// a trait and its derive from one path, as serde re-exports `serde_derive`.
pub(crate) const COMMON_DERIVE: &str = "campfire-common-derive";

#[test]
fn only_common_names_the_crate_of_its_derives() {
    let dependents: Vec<String> = crates()
        .into_iter()
        .filter(|krate| {
            let named = |line: &str| line.trim_start().starts_with(COMMON_DERIVE);
            krate.name != COMMON_DERIVE && krate.manifest.lines().any(named)
        })
        .map(|krate| krate.name)
        .collect();
    assert_eq!(dependents, ["campfire-common"]);
    let path = COMMON_DERIVE.replace('-', "_");
    let naming: Vec<String> = sources()
        .into_iter()
        // `common` re-exports them, and the rules hold the one place it may.
        .filter(|file| {
            let allowed = ["crates/base/common/", "crates/test/rules/"];
            let allowed = allowed.iter().any(|dir| file.path.starts_with(dir));
            !allowed && file.text.contains(&path)
        })
        .map(|file| file.path)
        .collect();
    assert_eq!(naming, Vec::<String>::new());
}

#[test]
fn the_design_marks_built_exactly_the_crates_of_the_workspace() {
    // The module table (02-engine-core.md, Modules) names each module in its first column and its
    // status in the second; a built module's crate is `campfire-<module>`, in a folder of `crates/`
    // or `checks/`.
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

/// The crates a sim's match runs on and whose code may read files or log: `store`, the one home
/// of files and threads, and the logs.
const NO_SIM_DEPENDENCY: [&str; 4] = ["campfire-log", "campfire-store", "log", "tracing"];

#[test]
fn the_sim_reads_no_file_and_logs_nothing_and_the_protocol_reads_no_file() {
    // Each crate of the workspace the release's required dependencies reach, the release among
    // them, requires none of the crates that read files or log, so a match runs the same on a
    // server, a client and a verifier, and its tick waits for no disk. An optional dependency is
    // a test build's.
    let manifests = manifests();
    let reached = sim_closure(&manifests);
    let mut found = Vec::new();
    for (name, manifest) in manifests.iter().filter(|(name, _)| reached.contains(name)) {
        for other in required_dependencies(manifest) {
            if NO_SIM_DEPENDENCY.contains(&other) {
                found.push(format!("{name} requires {other}"));
            }
        }
    }
    found.sort_unstable();
    assert_eq!(found, Vec::<String>::new());
    assert!(reached.contains("campfire-sim") && reached.contains("campfire-script"));
    assert!(!reached.contains("campfire-store"));
    // The protocol writes a log into a sink, which `net` gives over `store`.
    assert!(!crate_dependencies("session/protocol").contains(&"campfire-store".to_owned()));
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

/// The crates whose test infrastructure any crate's tests may take: `store`'s scratch and pinned
/// files, `log`'s check of the warnings, `common`'s allocation audit, `codec` for a type's encoded
/// form, `math` for the numbers a crate's values hold, and the test packages
/// (02-engine-core.md, Tests live at the top).
const TEST_INFRASTRUCTURE: [&str; 6] = [
    "campfire-codec",
    "campfire-common",
    "campfire-log",
    "campfire-math",
    "campfire-store",
    "campfire-test-packages",
];

/// The top crate of the session's and the network's tests: all of it is test code, its `tests/`
/// among it, and it depends on what they read.
const TOP_TEST_CRATE: &str = "campfire-harness";

/// The entries of `manifest`'s `internals` feature.
fn internals_entries(manifest: &str) -> Vec<&str> {
    let lines = CargoManifest::section(manifest, "features")
        .skip_while(|line| !line.starts_with("internals"));
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

/// The crates of the workspace each crate of `manifests` builds on, by its name: those it
/// depends on, and theirs, by its dependencies alone.
fn dependency_closures(manifests: &[(String, String)]) -> BTreeMap<&str, BTreeSet<&str>> {
    let workspace = |other: &str| manifests.iter().any(|(crate_name, _)| crate_name == other);
    manifests
        .iter()
        .map(|(name, _)| {
            let mut found = BTreeSet::new();
            let mut next = vec![name.as_str()];
            while let Some(at) = next.pop() {
                let (_, manifest) = manifests
                    .iter()
                    .find(|(crate_name, _)| crate_name == at)
                    .expect("a closure walks the workspace's crates");
                for other in
                    CargoManifest::section(manifest, "dependencies").map(CargoManifest::dependency)
                {
                    if workspace(other) && found.insert(other) {
                        next.push(other);
                    }
                }
            }
            (name.as_str(), found)
        })
        .collect()
}

/// What breaks the rule of the `internals` feature, by crate: a test build turns it on through
/// the dev-dependencies alone, so the code a test or a bench reaches has one gate,
/// `feature = "internals"`. A crate with the feature dev-depends on itself with it, and its
/// feature turns on the feature of each dependency that has it; each dev-dependency on a crate
/// with it turns it on, as a test of one crate uses another's helpers; and no crate has a `bench`
/// feature beside it. A dev-dependency on another crate of the workspace is test infrastructure;
/// a crate that is none of its dependencies, which only its tests read, and that builds on no
/// crate its own build lacks, so the test build gains that crate alone and holds no second copy
/// of one, as a crate that depends on it would bring; or, of a crate with no feature of its own,
/// as a binary, which cannot dev-depend on itself, a dependency whose helpers its tests read.
fn internals_problems(manifests: &[(String, String)]) -> Vec<String> {
    let closures = dependency_closures(manifests);
    let has = |manifest: &str, feature: &str| {
        CargoManifest::section(manifest, "features")
            .any(|line| line.starts_with(&format!("{feature} = ")))
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
        let dependencies: Vec<&str> = CargoManifest::section(manifest, "dependencies").collect();
        for line in &dependencies {
            let other = CargoManifest::dependency(line);
            let passed = line.contains(r#""internals""#)
                || entries.contains(&format!("{other}/internals").as_str())
                || entries.contains(&format!("{other}?/internals").as_str());
            if own && with_internals(other) && !passed {
                problems.push(format!("{name}'s internals leaves {other}'s off"));
            }
        }
        for line in CargoManifest::section(manifest, "dev-dependencies") {
            let other = CargoManifest::dependency(line);
            if with_internals(other) && !line.contains(r#""internals""#) {
                problems.push(format!("{name} dev-depends on {other} with no internals"));
            }
            let workspace = manifests.iter().any(|(crate_name, _)| crate_name == other);
            let depends = dependencies
                .iter()
                .any(|line| CargoManifest::dependency(line) == other);
            let within = workspace && closures[other].is_subset(&closures[name.as_str()]);
            let allowed = other == name
                || TEST_INFRASTRUCTURE.contains(&other)
                || if depends { !own } else { within };
            if workspace && !allowed {
                problems.push(format!(
                    "{name} dev-depends on {other}, past its tests' rule"
                ));
            }
            if workspace && other != name && closures[other].contains(name.as_str()) {
                problems.push(format!(
                    "{name} dev-depends on {other}, which builds on it: a second copy"
                ));
            }
        }
        let itself = CargoManifest::section(manifest, "dev-dependencies")
            .any(|line| CargoManifest::dependency(line) == name && line.contains(r#""internals""#));
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
    let mut on: Vec<String> = CargoManifest::section(own, "dependencies")
        .chain(CargoManifest::section(own, "dev-dependencies"))
        .filter(|line| line.contains(r#""internals""#))
        .map(|line| CargoManifest::dependency(line).to_owned())
        .collect();
    let mut at = 0;
    while let Some(next) = on.get(at) {
        let passed: Vec<String> = manifest(next)
            .map_or_default(internals_entries)
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
    // `a` has the feature and no dev-dependency on itself, and dev-depends on `b`, which builds on
    // nothing, and on `c`, which builds on `b`, past `a`'s build of nothing; `b` has it on itself,
    // and on `a` without it, and a `bench` feature beside it; `c` has none, and dev-depends on `b`,
    // its dependency, with it. `d` has the feature, leaves its dependency `b`'s off, and
    // dev-depends on it, on `c`, no dependency, which builds on `b` alone, and on
    // `campfire-store`, test infrastructure. `e` depends on `c` and dev-depends on `b`, which its
    // build holds through `c`. `f` dev-depends on `campfire-test-packages`, test infrastructure,
    // which builds on `f`, so `f`'s tests would hold two copies of it.
    let crates = [
        (
            "a",
            "[features]\ninternals = []\n\n[dev-dependencies]\nb.workspace = true\nc.workspace = true\n",
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
        (
            "e",
            concat!(
                "[features]\ninternals = []\n\n[dependencies]\nc.workspace = true\n\n",
                "[dev-dependencies]\nb = { workspace = true, features = [\"internals\"] }\n",
                "e = { workspace = true, features = [\"internals\"] }\n"
            ),
        ),
        (
            "f",
            "[dev-dependencies]\ncampfire-test-packages.workspace = true\n",
        ),
        ("campfire-test-packages", "[dependencies]\nf.workspace = true\n"),
    ]
    .map(|(name, manifest)| (name.to_owned(), manifest.to_owned()));
    // `a` turns no crate's on; `b` its own; `c` `b`'s; `d` its own, `a`'s through its own, `b`'s,
    // and `campfire-store`'s, which is not among them, and not `c`'s, which `d` passes on weakly.
    let reads = "x::internals::X; a::internals::A; b::internals::B; data::internals::D;";
    let weak = "c::internals::C;";
    let closures = dependency_closures(&crates);
    assert_eq!(
        ["a", "c", "e"].map(|name| closures[name].iter().copied().collect::<Vec<_>>()),
        [vec![], vec!["b"], vec!["b", "c"]]
    );
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
            "a dev-depends on c, past its tests' rule",
            "a dev-depends on itself with no internals",
            "b has a bench feature",
            "b dev-depends on a with no internals",
            "d's internals leaves b's off",
            "d dev-depends on b, past its tests' rule",
            "f dev-depends on campfire-test-packages, which builds on it: a second copy",
        ]
    );
    // The gate is spelled in parts, so this file holds none.
    let text = concat!(
        "#[cfg(feature = \"internals\")]\n#[cfg(any(test, feature = ",
        "\"internals\"))]\n"
    );
    assert_eq!(test_or_internals(text), [2]);
}

/// The required dependencies of `manifest` that `production`, its crate's production code, names
/// nowhere: a dependency that only code a gate holds uses is optional, and `internals` turns it on.
fn unnamed_dependencies<'a>(manifest: &'a str, production: &str) -> Vec<&'a str> {
    CargoManifest::section(manifest, "dependencies")
        .filter(|line| !line.contains("optional = true"))
        .map(CargoManifest::dependency)
        .filter(|name| !production.contains(&format!("{}::", name.replace('-', "_"))))
        .collect()
}

/// The code `view` keeps of each of `crates`, in their order: each source's that no crate nested
/// in its folder holds; all of the top test crate's, as all of it is test code.
fn code_by_crate(crates: &[Crate], view: fn(&SourceFile) -> Option<String>) -> Vec<String> {
    let mut code = vec![String::new(); crates.len()];
    for file in sources() {
        let Some(at) = holder(crates, &file.path) else {
            continue;
        };
        let kept = if crates[at].name == TOP_TEST_CRATE {
            Some(file.text)
        } else {
            view(&file)
        };
        if let Some(kept) = kept {
            code[at].push_str(&kept);
        }
    }
    code
}

/// The optional dependencies of `manifest` that `library`, its crate's library code, names
/// nowhere: one that only its tests name is a dev-dependency, as cargo's `unused_dependencies`
/// counts a use by the crate's integration tests.
fn tests_only_optional<'a>(manifest: &'a str, library: &str) -> Vec<&'a str> {
    CargoManifest::section(manifest, "dependencies")
        .filter(|line| line.contains("optional = true"))
        .map(CargoManifest::dependency)
        .filter(|name| !library.contains(&format!("{}::", name.replace('-', "_"))))
        .collect()
}

#[test]
fn a_required_dependency_is_named_by_production_code() {
    let crates = crates();
    let mut found = Vec::new();
    for (krate, production) in crates
        .iter()
        .zip(code_by_crate(&crates, SourceFile::production))
    {
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

/// The crates of `crates` whose build turns `internals` on through a normal dependency: one with
/// a required dependency that turns it on, and each that depends so on one of those, as cargo
/// unifies a dependency's features over the build.
fn unifying_internals(crates: &[Crate]) -> BTreeSet<&str> {
    fn required(krate: &Crate) -> Vec<&str> {
        CargoManifest::section(&krate.manifest, "dependencies")
            .filter(|line| !line.contains("optional = true"))
            .collect()
    }
    let mut found: BTreeSet<&str> = crates
        .iter()
        .filter(|krate| {
            required(krate)
                .iter()
                .any(|line| line.contains(r#""internals""#))
        })
        .map(|krate| krate.name.as_str())
        .collect();
    loop {
        let next: Vec<&str> = crates
            .iter()
            .filter(|krate| !found.contains(krate.name.as_str()))
            .filter(|krate| {
                required(krate)
                    .iter()
                    .any(|line| found.contains(CargoManifest::dependency(line)))
            })
            .map(|krate| krate.name.as_str())
            .collect();
        if next.is_empty() {
            return found;
        }
        found.extend(next);
    }
}

/// The paths `manifest`, the workspace's, lists as its `default-members`, in its order.
fn default_members(manifest: &str) -> Vec<&str> {
    manifest
        .lines()
        .skip_while(|line| *line != "default-members = [")
        .skip(1)
        .take_while(|line| *line != "]")
        .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
        .collect()
}

/// The crates of `crates` whose `internals` gates code of their own: a gate of the feature in
/// one of `sources`, as their paths under `source/` name them, or a target of the crate's that
/// requires the feature. A crate whose feature only passes the feature on gates none.
fn gating_internals<'a>(crates: &'a [Crate], sources: &[SourceFile]) -> BTreeSet<&'a str> {
    let gate = concat!("feature = ", "\"internals\"");
    let required = concat!("required-features = [", "\"internals\"]");
    let mut found: BTreeSet<&str> = crates
        .iter()
        .filter(|krate| krate.manifest.contains(required))
        .map(|krate| krate.name.as_str())
        .collect();
    for file in sources.iter().filter(|file| file.text.contains(gate)) {
        if let Some(at) = holder(crates, &file.path) {
            found.insert(crates[at].name.as_str());
        }
    }
    found
}

/// The dev-dependencies of `krate` that are also its dependencies, there to turn `internals` on,
/// on a crate of `gating`'s complement among `crates`: one whose feature gates no code of its
/// own, so the tests read no helper of it.
fn needless_internals<'a>(
    krate: &'a Crate,
    crates: &[Crate],
    gating: &BTreeSet<&str>,
) -> Vec<&'a str> {
    let dependencies: Vec<&str> = CargoManifest::section(&krate.manifest, "dependencies")
        .map(CargoManifest::dependency)
        .collect();
    CargoManifest::section(&krate.manifest, "dev-dependencies")
        .filter(|line| line.contains(r#""internals""#))
        .map(CargoManifest::dependency)
        .filter(|&other| other != krate.name && dependencies.contains(&other))
        .filter(|other| crates.iter().any(|known| known.name == *other) && !gating.contains(other))
        .collect()
}

#[test]
fn a_dev_dependency_turns_on_internals_that_gate_code() {
    let crates = crates();
    let gating = gating_internals(&crates, &sources());
    let found: Vec<String> = crates
        .iter()
        .flat_map(|krate| {
            needless_internals(krate, &crates, &gating)
                .into_iter()
                .map(move |other| format!("{} {other}", krate.name))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // `a` depends on `b`, `c` and `d`, and dev-depends on each with `internals`: `b`'s feature
    // only passes the feature on, so it is found; `c` gates code in a source and `d` requires the
    // feature for a target, so neither is. `e` dev-depends on `b` with no dependency on it, a
    // crate its tests read, which the rule leaves alone.
    let krate = |name: &str, manifest: &str| Crate {
        folder: name.to_owned(),
        path: format!("crates/{name}"),
        name: name.to_owned(),
        manifest: manifest.to_owned(),
    };
    let with = r#" = { workspace = true, features = ["internals"] }"#;
    let sample = [
        krate(
            "a",
            &format!(
                "[dependencies]\nb.workspace = true\nc.workspace = true\nd.workspace = true\n\n\
                 [dev-dependencies]\nb{with}\nc{with}\nd{with}\n"
            ),
        ),
        krate("b", "[features]\ninternals = [\"x/internals\"]\n"),
        krate("c", "[features]\ninternals = []\n"),
        krate(
            "d",
            "[features]\ninternals = []\n\n[[test]]\nname = \"t\"\nrequired-features = [\"internals\"]\n",
        ),
        krate("e", &format!("[dev-dependencies]\nb{with}\n")),
    ];
    let gated = SourceFile {
        path: "crates/c/src/lib.rs".to_owned(),
        text: concat!("#[cfg(feature = ", "\"internals\")]\nmod helpers;\n").to_owned(),
        gate: None,
    };
    let gating = gating_internals(&sample, &[gated]);
    assert_eq!(gating, BTreeSet::from(["c", "d"]));
    let found: Vec<Vec<&str>> = sample
        .iter()
        .map(|krate| needless_internals(krate, &sample, &gating))
        .collect();
    assert_eq!(found, [vec!["b"], vec![], vec![], vec![], vec![]]);
}

#[test]
fn the_default_members_are_the_build_that_ships() {
    // Each member but those that unify `internals`, by its folder under `source/`, sorted, so a
    // cargo command at the root with no `--workspace` builds what ships.
    let crates = crates();
    let unifying = unifying_internals(&crates);
    let mut expected: Vec<&str> = crates
        .iter()
        .filter(|krate| !unifying.contains(krate.name.as_str()))
        .map(|krate| krate.path.as_str())
        .collect();
    expected.sort_unstable();
    let manifest = read(source().join("Cargo.toml").to_str().unwrap());
    assert_eq!(default_members(&manifest), expected);
    assert!(unifying.contains("campfire-harness"));
    // A crate that turns `internals` on by a required dependency unifies it, and so does one that
    // depends on it; an optional one, or a dev-dependency, does not.
    let krate = |name: &str, dependencies: &str| Crate {
        folder: name.to_owned(),
        path: name.to_owned(),
        name: name.to_owned(),
        manifest: format!("[package]\nname = \"{name}\"\n\n[dependencies]\n{dependencies}"),
    };
    let sample = [
        krate(
            "a",
            "b = { workspace = true, features = [\"internals\"] }\n",
        ),
        krate("b", ""),
        krate("c", "a.workspace = true\n"),
        krate(
            "d",
            "b = { workspace = true, optional = true, features = [\"internals\"] }\n",
        ),
        krate(
            "e",
            "b.workspace = true\n\n[dev-dependencies]\nb = { workspace = true, features = [\"internals\"] }\n",
        ),
    ];
    assert_eq!(unifying_internals(&sample), BTreeSet::from(["a", "c"]));
    let listed = "[workspace]\ndefault-members = [\n    \"crates/a\",\n    \"checks/b\",\n]\n";
    assert_eq!(default_members(listed), ["crates/a", "checks/b"]);
}

#[test]
fn an_optional_dependency_is_named_by_library_code() {
    let crates = crates();
    let mut found = Vec::new();
    for (krate, library) in crates
        .iter()
        .zip(code_by_crate(&crates, SourceFile::library))
    {
        for name in tests_only_optional(&krate.manifest, &library) {
            found.push(format!("{} {name}", krate.name));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // An optional dependency that only a test names is found: in a `tests/` file, a `tests.rs`, or
    // an item `#[cfg(test)]` holds; one that `internals` code or a bench names is not, nor a
    // required one.
    let manifest = "[package]\nname = \"a\"\n\n[dependencies]\nb.workspace = true\nc-d = { workspace = true, optional = true }\ne = { workspace = true, optional = true }\n";
    let library = |path: &str, text: &str| {
        let file = SourceFile {
            path: path.to_owned(),
            text: text.to_owned(),
            gate: None,
        };
        file.library().unwrap_or_default()
    };
    let gated = "#[cfg(feature = \"internals\")]\nfn f() {\n    c_d::f();\n}\n";
    assert_eq!(
        tests_only_optional(manifest, &library("src/a.rs", gated)),
        ["e"]
    );
    assert_eq!(
        tests_only_optional(manifest, &library("src/bench.rs", "use e::X;")),
        ["c-d"]
    );
    for (path, text) in [
        ("tests/a.rs", "use c_d::X; use e::Y;"),
        ("src/tests.rs", "use c_d::X; use e::Y;"),
        (
            "src/a.rs",
            "#[cfg(test)]\nfn t() {\n    c_d::f();\n    e::g();\n}\n",
        ),
    ] {
        let found = tests_only_optional(manifest, &library(path, text));
        assert_eq!(found, ["c-d", "e"], "{path}");
    }
}

#[test]
fn the_name_checks_fail_on_what_breaks_them() {
    // A proxy's source that names another proxy's call or one of no proxy's, but not its own.
    let [store, codec, _] = &PROXIES;
    let text = "use std::fs::File;\nlet map: HashMap<u8, u8>;\nlet n = 1;";
    assert_eq!(foreign_names(codec, text), ["std::fs", "HashMap"]);
    assert_eq!(
        foreign_names(store, "use std::fs::File;"),
        Vec::<&str>::new()
    );
}
