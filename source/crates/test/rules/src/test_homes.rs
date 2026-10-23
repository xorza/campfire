use std::collections::{BTreeMap, BTreeSet};

use crate::allowlist::Allowlist;
use crate::dev_dependencies::is_test_support;
use crate::source_file::SourceFile;
use crate::workspace::{Crate, DevEdges, Workspace, holder, test_build};

/// The crates of `crates` whose paths `code` names, `campfire_x::` for `campfire-x`.
fn named_crates<'a>(crates: &'a [Crate], code: &str) -> BTreeSet<&'a str> {
    crates
        .iter()
        .filter(|krate| {
            let path = format!("{}::", krate.name.replace('-', "_"));
            code.match_indices(&path).any(|(at, _)| {
                !code[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|before| before.is_alphanumeric() || before == '_')
            })
        })
        .map(|krate| krate.name.as_str())
        .collect()
}

/// The file of the module of an integration test's target at `dir`, under `root`, the target's
/// directory: `dir/mod.rs` or `dir.rs`, or the target's root for `root` itself.
fn module_file<'s>(
    files: &BTreeMap<&str, &'s SourceFile>,
    root: &str,
    dir: &str,
) -> Option<&'s SourceFile> {
    let paths = if dir == root {
        vec![format!("{root}/mod.rs")]
    } else {
        vec![format!("{dir}/mod.rs"), format!("{dir}.rs")]
    };
    paths
        .iter()
        .find_map(|path| files.get(path.as_str()).copied())
}

/// The files of an integration test's target that `file`, one of them under `root`, the target's
/// directory, reads its helpers from: the parent module for `super::`, and the module or the root
/// for `crate::`, each with the files it reads from in turn.
fn helper_files<'s>(
    files: &BTreeMap<&str, &'s SourceFile>,
    root: &str,
    file: &'s SourceFile,
) -> Vec<&'s SourceFile> {
    let mut found: Vec<&SourceFile> = Vec::new();
    let mut next = vec![file];
    while let Some(at) = next.pop() {
        let code = at.test_code().unwrap_or_default();
        let dir = at.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let mut read = Vec::new();
        if code.contains("super::") {
            let parent = if at.path.ends_with("/mod.rs") {
                dir.rsplit_once('/').map_or("", |(parent, _)| parent)
            } else {
                dir
            };
            read.extend(module_file(files, root, parent));
        }
        for (start, _) in code.match_indices("crate::") {
            let rest = &code[start + "crate::".len()..];
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let module = module_file(files, root, &format!("{root}/{}", &rest[..end]));
            read.extend(module.or_else(|| module_file(files, root, root)));
        }
        for helper in read {
            if helper.path != file.path && !found.iter().any(|seen| seen.path == helper.path) {
                found.push(helper);
                next.push(helper);
            }
        }
    }
    found
}

/// The crates whose items the tests of `taker`, a crate of `manifests`, reach: the crate, what its
/// test build holds, and each crate of the workspace it may take as test support.
fn reach<'a>(manifests: &'a [Crate], taker: &'a str) -> BTreeSet<&'a str> {
    let mut reached = test_build(manifests, taker, DevEdges::Left);
    reached.insert(taker);
    let support = manifests
        .iter()
        .map(|krate| krate.name.as_str())
        .filter(|other| is_test_support(manifests, taker, other));
    reached.extend(support);
    reached
}

/// Each test file of `sources`, one that holds a test, that a crate of `crates` below its own
/// could hold, `manifests` their manifests, with the lowest such crates
/// (02-engine-core.md, One rule of test homes): the crates a file names, with those its target's
/// modules that it reads from name, and its own where a unit test's file reads `crate::` or
/// `super::` or a test runs the crate's binary, lie in that crate's reach.
fn misplaced_tests(
    crates: &[Crate],
    sources: &[SourceFile],
    kept: &Allowlist<&str>,
) -> Vec<String> {
    let files: BTreeMap<&str, &SourceFile> = sources
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut reaches: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut found = Vec::new();
    for file in sources {
        let (Some(code), Some(at)) = (file.test_code(), holder(crates, &file.path)) else {
            continue;
        };
        if !code.contains("#[test]") {
            continue;
        }
        let own = &crates[at];
        let tests = format!("{}/tests", own.path);
        let mut named = named_crates(crates, code);
        if file.path.starts_with(&format!("{tests}/")) {
            for helper in helper_files(&files, &tests, file) {
                named.extend(named_crates(crates, helper.test_code().unwrap_or_default()));
            }
        } else if code.contains("crate::") || code.contains("super::") {
            named.insert(own.name.as_str());
        }
        if code.contains("CARGO_BIN_EXE_") {
            named.insert(own.name.as_str());
        }
        let below: BTreeSet<&str> = test_build(crates, &own.name, DevEdges::Left)
            .into_iter()
            .filter(|other| crates.iter().any(|krate| krate.name == *other))
            .collect();
        let homes: Vec<&str> = below
            .iter()
            .copied()
            .filter(|lower| {
                let reached = reaches.entry(lower).or_insert_with(|| reach(crates, lower));
                named.is_subset(reached)
            })
            .collect();
        let lowest: Vec<&str> = homes
            .iter()
            .copied()
            .filter(|home| {
                let under = test_build(crates, home, DevEdges::Left);
                !homes.iter().any(|other| under.contains(other))
            })
            .collect();
        if !lowest.is_empty() && !kept.allows(|path| *path == file.path) {
            found.push(format!("{} could live in {}", file.path, lowest.join(", ")));
        }
    }
    found
}

#[test]
fn a_test_lives_in_the_lowest_crate_that_can_hold_it() {
    let workspace = Workspace::read();
    let kept = Allowlist::new(&KEPT_BY_DECISION);
    let misplaced = misplaced_tests(workspace.crates(), workspace.sources(), &kept);
    assert_eq!(misplaced, Vec::<String>::new());
    assert_eq!(kept.unused(), Vec::<&&str>::new());
    // `high` depends on `low`. Of its test files, `a.rs` and `y/tests.rs` name `low` alone, so
    // `low` can hold them; `b.rs` names `high`, `c.rs` reads its helpers from the target's root,
    // which names `high`, `x/tests.rs` reads `super::`, and `e.rs` runs `high`'s binary, so each
    // stays; `d.rs` holds no test.
    let crates = [
        Crate::sample("high", "[dependencies]\nlow.workspace = true\n"),
        Crate::sample("low", ""),
    ];
    let test = "#[test]\nfn t() {}\n";
    let sources = [
        (
            "crates/high/tests/mod.rs",
            "mod a;\nuse high::Helper;\n".to_owned(),
        ),
        ("crates/high/tests/a.rs", format!("use low::X;\n{test}")),
        ("crates/high/tests/b.rs", format!("use high::Y;\n{test}")),
        (
            "crates/high/tests/c.rs",
            format!("use super::Helper;\nuse low::X;\n{test}"),
        ),
        ("crates/high/tests/d.rs", "use low::X;\n".to_owned()),
        (
            "crates/high/tests/e.rs",
            format!("const BIN: &str = env!(\"CARGO_BIN_EXE_high\");\n{test}"),
        ),
        (
            "crates/high/src/x/tests.rs",
            format!("use super::*;\n{test}"),
        ),
        ("crates/high/src/y/tests.rs", format!("use low::Z;\n{test}")),
    ]
    .map(|(path, text)| SourceFile::new(path, text));
    assert_eq!(
        misplaced_tests(&crates, &sources, &Allowlist::new(&[])),
        [
            "crates/high/tests/a.rs could live in low",
            "crates/high/src/y/tests.rs could live in low",
        ]
    );
}

/// The test files a decision keeps above their lowest home, each with its decision.
const KEPT_BY_DECISION: [&str; 1] = [
    // The stack's allocations are audited in one binary (11-net-stack/testing.md, One allocation
    // binary, on the workspace's audit).
    "crates/network/net-stack/tests/alloc/fixtures/writer.rs",
];
