//! A mode that runs a rules package's script as its own.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use campfire_common::MapName;
use campfire_package::{
    LoadError, LoadProblem, ModePackages, PackageDir, PackageName, PackageRef, ScriptProblem,
};
use campfire_script::ScriptId;

const RULED: &str = "modes/ruled";
const MODE_DATA: &str = "modes/ruled/data/mode.toml";

/// The test packages, read from disk once.
fn test_files() -> &'static BTreeMap<PathBuf, Vec<u8>> {
    static FILES: OnceLock<BTreeMap<PathBuf, Vec<u8>>> = OnceLock::new();
    FILES.get_or_init(|| PackageDir::workspace_tree("test"))
}

/// The ruled mode, with each of `edits` made first: a file's text replaced by another, or, from
/// no text, a file made of it.
fn ruled(edits: &[(&str, &str, &str)]) -> Result<ModePackages, LoadError> {
    let mut files = test_files().clone();
    for &(file, from, to) in edits {
        let path = PathBuf::from(file);
        if from.is_empty() {
            files.insert(path, to.as_bytes().to_vec());
            continue;
        }
        let text = String::from_utf8(files[&path].clone()).unwrap();
        assert!(text.contains(from), "{file}: {from}");
        files.insert(path, text.replacen(from, to, 1).into_bytes());
    }
    let dir = PackageDir::in_memory(Arc::new(PackageDir::reindexed(files)), RULED);
    ModePackages::from_package_dir(&dir, &MapName::new("stand").unwrap())
}

#[test]
fn a_mode_runs_its_rules_package_s_script_and_fails_on_one_it_does_not_hold() {
    // The mode holds no script: the rules package's is the first a match compiles.
    let packages = ruled(&[]).unwrap();
    assert_eq!(packages.mode_script(), ScriptId::nth(0));
    let problem = |result: Result<ModePackages, LoadError>| {
        let error = result.unwrap_err();
        (error.package, *error.problem)
    };
    let named = |name| PackageRef::Name(PackageName::new(name).unwrap());
    // A package the mode does not depend on, or one that is no rules package, is none to run.
    let (at, refused) = problem(ruled(&[(MODE_DATA, "test-rules", "test-other")]));
    assert_eq!(at, named("test-ruled"));
    assert!(matches!(refused, LoadProblem::ScriptPackage(name) if name == "test-other"));
    // A path the rules package lacks fails that package's load with it.
    let (at, refused) = problem(ruled(&[(
        MODE_DATA,
        "scripts/mode.rhai",
        "scripts/rule.rhai",
    )]));
    assert_eq!(at, named("test-rules"));
    assert!(matches!(
        refused,
        LoadProblem::Script { path, problem: ScriptProblem::Missing } if path.as_str() == "scripts/rule.rhai"
    ));
    // A mode that runs its own script leaves the rules package's one no data names.
    let script = "script = { package = \"test-rules\", path = \"scripts/mode.rhai\" }";
    let (at, refused) = problem(ruled(&[
        (
            "modes/ruled/scripts/own.rhai",
            "",
            "fn on_match_start(ctx) {}",
        ),
        (MODE_DATA, script, "script = \"scripts/own.rhai\""),
    ]));
    assert_eq!(at, named("test-rules"));
    assert!(matches!(
        refused,
        LoadProblem::Script { path, problem: ScriptProblem::Unreferenced } if path.as_str() == "scripts/mode.rhai"
    ));
}
