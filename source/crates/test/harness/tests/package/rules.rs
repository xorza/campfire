//! A mode that runs a rules package's script as its own.

use campfire_common::DeclaredName;
use campfire_package::{
    DataOverride, LoadError, LoadProblem, ModePackages, PackageName, PackageRef, ScriptProblem,
};
use campfire_script::{ListPlace, ModeScripts, ScriptId, StateValue};
use campfire_sim::Capability;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_values::{Named, Param};

const MODE_DATA: &str = "modes/ruled/data/mode.toml";
/// The ruled mode's list: the rules package's script alone.
const LIST: &str = "scripts = [{ package = \"test-rules\", path = \"scripts/mode.rhai\" }]";

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
    assert_eq!(packages.mode_scripts(), ModeScripts::one(ScriptId::nth(0)));
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
    let (at, refused) = problem(ruled(&[
        (
            "modes/ruled/scripts/own.rhai",
            Edit::Create("fn on_match_start(ctx) {}"),
        ),
        (
            MODE_DATA,
            Edit::Replace(LIST, "scripts = [\"scripts/own.rhai\"]"),
        ),
    ]));
    assert_eq!(at, named("test-rules"));
    assert!(matches!(
        refused,
        LoadProblem::Script { path, problem: ScriptProblem::Unreferenced } if path.as_str() == "scripts/mode.rhai"
    ));
}

#[test]
fn a_mode_lists_scripts_in_order_and_only_its_own_ends_the_match() {
    const OWN: &str = "modes/ruled/scripts/own.rhai";
    let problem = |result: Result<ModePackages, LoadError>| {
        let error = result.unwrap_err();
        (error.package, *error.problem)
    };
    let named = |name| PackageRef::Name(PackageName::new(name).unwrap());
    // The rules package's script, marked the mode's own, then the mode's: the match compiles
    // the mode's first, as script 0, and the rules package's next.
    let own = (OWN, Edit::Create("fn on_match_start(ctx) {}"));
    let listed = ruled(&[
        own,
        (
            MODE_DATA,
            Edit::Replace(
                LIST,
                "scripts = [{ package = \"test-rules\", path = \"scripts/mode.rhai\", mode = \
                 true }, \"scripts/own.rhai\"]",
            ),
        ),
    ])
    .unwrap();
    let scripts = ModeScripts::new([ScriptId::nth(1), ScriptId::nth(0)], ListPlace::FIRST);
    assert_eq!(listed.mode_scripts(), scripts);
    // Marked the other way, the rules package's script is a rules script, and its `ctx.win`
    // fails its package's load: only the mode's own script ends the match.
    let (at, refused) = problem(ruled(&[
        own,
        (
            MODE_DATA,
            Edit::Replace(
                LIST,
                "scripts = [{ package = \"test-rules\", path = \"scripts/mode.rhai\" }, { path \
                 = \"scripts/own.rhai\", mode = true }]",
            ),
        ),
    ]));
    assert_eq!(at, named("test-rules"));
    assert!(
        matches!(&refused, LoadProblem::Script { path, problem: ScriptProblem::OwnScriptOnly(name) } if path.as_str() == "scripts/mode.rhai" && name == "win"),
        "{refused:?}"
    );
    // So does its read of the mode's state, of a field the mode declares.
    let (_, refused) = problem(ruled(&[
        own,
        (
            MODE_DATA,
            Edit::Append("[state]\nphase = { type = \"int\" }\n"),
        ),
        (
            "rules/scripts/mode.rhai",
            Edit::Replace("ctx.win(ctx.teams[0]);", "let phase = ctx.state.phase;"),
        ),
        (
            MODE_DATA,
            Edit::Replace(
                LIST,
                "scripts = [{ package = \"test-rules\", path = \"scripts/mode.rhai\" }, { path \
                 = \"scripts/own.rhai\", mode = true }]",
            ),
        ),
    ]));
    assert!(
        matches!(&refused, LoadProblem::Script { problem: ScriptProblem::OwnScriptOnly(name), .. } if name == "state"),
        "{refused:?}"
    );
    // A rules package the list names that the mode does not depend on fails as the one script.
    let (at, refused) = problem(ruled(&[
        own,
        (
            MODE_DATA,
            Edit::Replace(
                LIST,
                "scripts = [{ package = \"test-other\", path = \"scripts/mode.rhai\" }, { path \
                 = \"scripts/own.rhai\", mode = true }]",
            ),
        ),
    ]));
    assert_eq!(at, named("test-ruled"));
    assert!(matches!(refused, LoadProblem::ScriptPackage(name) if name == "test-other"));
}

#[test]
fn a_pure_hook_in_two_scripts_of_a_mode_s_list_fails_the_second() {
    // The skirmish lists Zero Hour's script, its own, then two of the mode's that each weigh
    // damage: the second fails.
    const MODE: &str = "test/modes/skirmish/data/mode.toml";
    let weigh = Edit::Create("fn calc_damage(ctx, d) { d.amount }");
    let mut tree = PackageTree::of(TestMode::Skirmish);
    tree.edit("test/modes/skirmish/scripts/a.rhai", weigh);
    let list = "scripts = [{ package = \"zero-hour\", path = \"scripts/mode.rhai\" }]";
    let one = "scripts = [{ package = \"zero-hour\", path = \"scripts/mode.rhai\", mode = true }, \
               \"scripts/a.rhai\"]";
    let mut first = tree.clone();
    first.edit(MODE, Edit::Replace(list, one));
    first.load().unwrap();
    let two = "scripts = [{ package = \"zero-hour\", path = \"scripts/mode.rhai\", mode = true }, \
               \"scripts/a.rhai\", \"scripts/b.rhai\"]";
    tree.edit("test/modes/skirmish/scripts/b.rhai", weigh);
    tree.edit(MODE, Edit::Replace(list, two));
    let error = tree.load().unwrap_err();
    assert_eq!(
        error.package,
        PackageRef::Name(PackageName::new("test-skirmish").unwrap())
    );
    assert!(
        matches!(&*error.problem, LoadProblem::Script { path, problem: ScriptProblem::PureHookTwice(name) } if path.as_str() == "scripts/b.rhai" && name == "calc_damage"),
        "{:?}",
        error.problem
    );
}

#[test]
fn a_rules_package_s_data_lies_under_the_mode_s_and_its_needs_refuse_a_mode_that_lacks_them() {
    const RULES_DATA: &str = "rules/data/rules.toml";
    const RULES_MANIFEST: &str = "rules/manifest.toml";
    // The rules give `round` and `phase`, and the mode its own `phase`, a record of its own: the
    // mode keeps the rules' `round` and its own `phase`, and the load lists the one record both
    // give, which the mode's value takes.
    let rules = "[state]\nround = { type = \"int\" }\nphase = { type = \"string\" }\n";
    let phase = "[state]\nphase = { type = \"int\", default = 2 }\n";
    let packages = ruled(&[
        (RULES_DATA, Edit::Create(rules)),
        (MODE_DATA, Edit::Append(phase)),
    ])
    .unwrap();
    let state = &packages.data().state;
    let initial = |name: &str| {
        state[&DeclaredName::new(name).unwrap()]
            .decl
            .initial()
            .clone()
    };
    assert_eq!(
        [initial("phase"), initial("round")],
        [StateValue::Int(2), StateValue::Int(0)]
    );
    let by = PackageName::new("test-ruled").unwrap();
    assert_eq!(
        packages.overrides(),
        [DataOverride {
            record: "state.phase".to_owned(),
            by
        }]
    );
    let problem = |result: Result<ModePackages, LoadError>| *result.unwrap_err().problem;
    // A rules package's data gives no script and no content, which are the mode's own.
    for (key, data) in [
        ("scripts", "scripts = [\"scripts/mode.rhai\"]\n"),
        ("units", "[units]\n"),
    ] {
        let refused = problem(ruled(&[(RULES_DATA, Edit::Create(data))]));
        assert!(
            matches!(&refused, LoadProblem::RulesKey(given) if given == key),
            "{refused:?}"
        );
    }
    // The mode declares no capability and runs at 30 Hz: rules that need combat, or 60 Hz, refuse
    // it, and rules that run at 20 to 30 Hz take it.
    let needs = |needs: &'static str| (RULES_MANIFEST, Edit::Append(needs));
    let rules_name = PackageName::new("test-rules").unwrap();
    assert!(matches!(
        problem(ruled(&[needs("capabilities = [\"combat\"]\n")])),
        LoadProblem::RulesCapability { package, capability: Capability::Combat } if package == rules_name
    ));
    assert!(matches!(
        problem(ruled(&[needs("tick_hz = { min = 60, max = 60 }\n")])),
        LoadProblem::RulesRate(package) if package == rules_name
    ));
    assert!(ruled(&[needs("tick_hz = { min = 20, max = 30 }\n")]).is_ok());
}

#[test]
fn a_marker_param_the_rules_declare_loads_on_a_map_whose_markers_lack_it() {
    // Zero Hour's script reads each start marker's `name`, which its rules data declares: the
    // skirmish loads on a map whose markers hold no param, and fails with the declaration gone.
    const MAP: &str = "test/modes/skirmish/map/field/map.toml";
    let params: Vec<String> = (1..=3)
        .map(|n| format!("params = {{ name = \"Player_{n}_Start\" }}\n"))
        .collect();
    let mut tree = PackageTree::of(TestMode::Skirmish);
    for param in &params {
        tree.edit(MAP, Edit::Replace(param, ""));
    }
    assert!(tree.clone().load().is_ok());
    tree.edit(
        "zero-hour/data/rules.toml",
        Edit::Replace("marker_params = [\"name\"]\n", ""),
    );
    let refused = *tree.load().unwrap_err().problem;
    assert!(
        matches!(&refused, LoadProblem::Unknown { of: Param::NAMES, name, .. } if name == "name"),
        "{refused:?}"
    );
}

#[test]
fn a_victory_of_a_tag_no_type_holds_fails_the_load() {
    // Zero Hour's rules count the tag `counts_for_victory`; one the match does not declare fails.
    let mut tree = PackageTree::of(TestMode::Skirmish);
    tree.edit(
        "zero-hour/data/rules.toml",
        Edit::Replace("counts = \"counts_for_victory\"", "counts = \"crowns\""),
    );
    let refused = *tree.load().unwrap_err().problem;
    assert!(
        matches!(&refused, LoadProblem::Unknown { name, .. } if name == "crowns"),
        "{refused:?}"
    );
}
