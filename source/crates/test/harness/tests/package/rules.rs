//! A mode that runs a rules package's script as its own.

use campfire_package::{
    LoadError, LoadProblem, ModePackages, PackageName, PackageRef, ScriptProblem,
};
use campfire_script::ScriptId;
use campfire_test_packages::{Edit, PackageTree, TestMode};

const MODE_DATA: &str = "modes/ruled/data/mode.toml";

/// The ruled mode, with each of `edits` made first.
fn ruled(edits: &[(&str, Edit<'_>)]) -> Result<ModePackages, LoadError> {
    let mut tree = PackageTree::of(TestMode::Ruled);
    for &(file, edit) in edits {
        tree.edit(file, edit);
    }
    tree.load()
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
    let (at, refused) = problem(ruled(&[(
        MODE_DATA,
        Edit::Replace("test-rules", "test-other"),
    )]));
    assert_eq!(at, named("test-ruled"));
    assert!(matches!(refused, LoadProblem::ScriptPackage(name) if name == "test-other"));
    // A path the rules package lacks fails that package's load with it.
    let (at, refused) = problem(ruled(&[(
        MODE_DATA,
        Edit::Replace("scripts/mode.rhai", "scripts/rule.rhai"),
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
            Edit::Create("fn on_match_start(ctx) {}"),
        ),
        (
            MODE_DATA,
            Edit::Replace(script, "script = \"scripts/own.rhai\""),
        ),
    ]));
    assert_eq!(at, named("test-rules"));
    assert!(matches!(
        refused,
        LoadProblem::Script { path, problem: ScriptProblem::Unreferenced } if path.as_str() == "scripts/mode.rhai"
    ));
}
