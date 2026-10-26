//! A mode that runs a rules package's script as its own.

use campfire_common::DeclaredName;
use campfire_mode::ModeParam;
use campfire_package::{
    DataOverride, LoadError, LoadProblem, ModePackages, PackageRef, Place, ScriptProblem,
};
use campfire_script::{ListPlace, ModeScripts, ScriptId, StateValue};
use campfire_sim::Capability;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_values::{Named, PackageName, Param, Scalar};

const MODE_DATA: &str = "modes/ruled/data/mode.toml";
/// The ruled mode's list: the rules package's script alone.
const LIST: &str = "scripts = [{ package = \"test-rules\", path = \"scripts/mode.rhai\" }]";

/// The ruled mode, with each of `edits` made first.
fn ruled(edits: &[(&str, Edit<'_>)]) -> Result<ModePackages, LoadError> {
    PackageTree::of(TestMode::Ruled)
        .edit_all(edits.iter().copied())
        .load()
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

/// The lane's towers think by the shared AI of `test-ai`, a rules package the lane depends on; a
/// unit type's AI of a package the mode does not depend on, or of one that is no rules package,
/// fails the load.
#[test]
fn a_unit_type_s_ai_is_a_script_of_a_rules_package_the_mode_depends_on() {
    const UNITS: &str = "modes/lane/data/units.toml";
    let lane = TestMode::Lane.packages();
    let names: Vec<_> = (lane.packages())
        .map(|view| view.package.header.name.as_str())
        .collect();
    assert!(names.contains(&"test-ai"), "{names:?}");
    for other in ["test-other", "hero-runner"] {
        let refused = PackageTree::of(TestMode::Lane)
            .edit(
                UNITS,
                Edit::Replace(
                    "\"test-ai\", path = \"scripts/tower_ai",
                    &format!("\"{other}\", path = \"scripts/tower_ai"),
                ),
            )
            .load()
            .unwrap_err();
        assert_eq!(
            refused.package,
            PackageRef::Name(PackageName::new("test-lane").unwrap())
        );
        assert!(
            matches!(&*refused.problem, LoadProblem::ScriptPackage(name) if name == other),
            "{other}: {refused:?}"
        );
    }
}

#[test]
fn a_mode_names_its_bots_each_a_script_of_a_rules_package_or_its_own() {
    const BOT: &str = "rules/scripts/bot.rhai";
    let problem = |result: Result<ModePackages, LoadError>| *result.unwrap_err().problem;
    let bot = |script: &str, rest: &str| {
        format!("[bots.raider]\nscript = {script}\nthink_ms = 250\n{rest}")
    };
    let rules = "{ package = \"test-rules\", path = \"scripts/bot.rhai\" }";
    let think = |body: &'static str| (BOT, Edit::Create(body));
    // The rules package's `bot.rhai`, which the match compiles before its `mode.rhai`, and which
    // no data but the bot names: the mode names it `raider`, with its period and its param
    // `army`, which its script reads.
    let army = think("fn on_bot_think(ctx) { let army = ctx.params.army; }");
    let named = bot(rules, "params = { army = 4 }\n");
    let packages = ruled(&[army, (MODE_DATA, Edit::Append(&named))]).unwrap();
    let raider = packages.bot(&DeclaredName::new("raider").unwrap()).unwrap();
    assert_eq!(raider.script, ScriptId::nth(0));
    assert_eq!(packages.mode_scripts(), ModeScripts::one(ScriptId::nth(1)));
    assert_eq!(raider.think_ms.get(), 250);
    let army = DeclaredName::new("army").unwrap();
    assert_eq!(raider.params[&army], ModeParam::Value(Scalar::Int(4)));
    assert!(
        packages
            .bot(&DeclaredName::new("walker").unwrap())
            .is_none()
    );
    // A param the bot lacks, and a hook of the mode's, fail the script's load.
    let unknown = think("fn on_bot_think(ctx) { let navy = ctx.params.navy; }");
    let refused = problem(ruled(&[unknown, (MODE_DATA, Edit::Append(&named))]));
    assert!(
        matches!(&refused, LoadProblem::Unknown { of: Param::NAMES, name, .. } if name == "navy"),
        "{refused:?}"
    );
    let mode_hook = think("fn on_match_start(ctx) {}");
    let refused = problem(ruled(&[mode_hook, (MODE_DATA, Edit::Append(&named))]));
    assert!(
        matches!(&refused, LoadProblem::Script { problem: ScriptProblem::UnknownHook(name), .. } if name == "on_match_start"),
        "{refused:?}"
    );
    // A bot of a package the mode does not depend on is none to run, and a bot of no period, or
    // of a key a bot lacks, fails the mode's data.
    let other = bot(
        "{ package = \"test-other\", path = \"scripts/bot.rhai\" }",
        "",
    );
    let refused = problem(ruled(&[(MODE_DATA, Edit::Append(&other))]));
    assert!(matches!(refused, LoadProblem::ScriptPackage(name) if name == "test-other"));
    let still = think("fn on_bot_think(ctx) {}");
    for named in [
        bot(rules, "").replace("think_ms = 250", "think_ms = 0"),
        bot(rules, "army = 4\n"),
    ] {
        let refused = problem(ruled(&[still, (MODE_DATA, Edit::Append(&named))]));
        assert!(
            matches!(refused, LoadProblem::Content(_)),
            "{named}: {refused:?}"
        );
    }
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
    const MODE: &str = "test/engine/modes/skirmish/data/mode.toml";
    let weigh = Edit::Create("fn calc_damage(ctx, d) { d.amount }");
    let mut tree = PackageTree::of(TestMode::Skirmish);
    tree.edit("test/engine/modes/skirmish/scripts/a.rhai", weigh);
    let list = "scripts = [{ package = \"zero-hour\", path = \"scripts/mode.rhai\" }]";
    let one = "scripts = [{ package = \"zero-hour\", path = \"scripts/mode.rhai\", mode = true }, \
               \"scripts/a.rhai\"]";
    let mut first = tree.clone();
    first.edit(MODE, Edit::Replace(list, one));
    first.load().unwrap();
    let two = "scripts = [{ package = \"zero-hour\", path = \"scripts/mode.rhai\", mode = true }, \
               \"scripts/a.rhai\", \"scripts/b.rhai\"]";
    tree.edit("test/engine/modes/skirmish/scripts/b.rhai", weigh);
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
    // Zero Hour's script reads each start marker's `player_slot`, which its rules data declares:
    // the skirmish loads on a map whose markers hold no param, and fails with the declaration
    // gone.
    const MAP: &str = "test/engine/modes/skirmish/map/field/map.toml";
    let params: Vec<String> = (1..=3)
        .map(|n| {
            format!(
                "params = {{ name = \"Player_{n}_Start\", player_slot = {} }}\n",
                n - 1
            )
        })
        .collect();
    let mut tree = PackageTree::of(TestMode::Skirmish);
    for param in &params {
        tree.edit(MAP, Edit::Replace(param, ""));
    }
    assert!(tree.clone().load().is_ok());
    tree.edit(
        "zero-hour/data/rules.toml",
        Edit::Replace(", \"player_slot\"]", "]"),
    );
    let refused = *tree.load().unwrap_err().problem;
    assert!(
        matches!(&refused, LoadProblem::Unknown { of: Param::NAMES, name, .. } if name == "player_slot"),
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

/// A victory and a bounty follow the match's deaths, which combat makes: a mode without combat,
/// as the ruled mode is, that gives either fails its load with the section.
#[test]
fn a_victory_and_a_bounty_need_combat() {
    for (key, section) in [
        ("victory", "[victory]\ncounts = \"hero\""),
        ("bounty", "[bounty]\nkiller = \"enemies:hero\""),
    ] {
        let refused = ruled(&[(MODE_DATA, Edit::Append(section))]).unwrap_err();
        assert!(
            matches!(
                *refused.problem,
                LoadProblem::Undeclared {
                    capability: Capability::Combat,
                    at: Place::Section(at),
                } if at == key
            ),
            "{key}: {:?}",
            refused.problem
        );
    }
}
