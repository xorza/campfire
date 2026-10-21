//! Each flaw a package can have fails the load of the MOBA packages with its own problem.

use crate::flaws::{Flaw, fails_each};
use campfire_actions::{ActionProblem, EffectRule, ListHook};
use campfire_package::{LoadProblem, Place, ScriptProblem};
use campfire_production::ProductionDataError;
use campfire_script::CtxMisuse;
use campfire_script::internals::Undefined;
use campfire_sim::Capability;
use campfire_test_packages::Edit;
use campfire_values::{DamageKind, EngineEnum, ModifierId, Named, Param};

use crate::moba::{edited, load};
use crate::module_rules::{entry, rule_at, rule_of};

/// A flaw, the package it is in, and the problem it fails the load with.
use crate::mode_package::{
    CHAIN_FIRE, CINDER, CREEP_AI, HUSK, LASH_OUT, MANIFEST, MODE, MODE_DATA, MODE_SCRIPT,
    PRODUCTION, RIME, read_fails, undefined,
};

/// Flaws of an effect's amount, a handle, a state field and a script's compile.
pub(super) static SCRIPT_FLAWS: &[Flaw] = &[
    // An amount of i64::MAX, 2⁶³ − 1, is past what a sim number holds, whose most is
    // (2⁶³ − 1) / 2²⁴.
    Flaw::of(
        RIME,
        Edit::Set(
            "actions.fan_of_frost.on_hit",
            r#"[{ damage = { amount = 9223372036854775807, kind = "true" } }]"#,
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "fan_of_frost"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Overflow,
                },
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set("actions.fan_of_frost.params.slow_ms", r#""2000.5""#),
        "hero-rime",
        |problem| {
            rule_of(problem, |_, error: &ActionProblem| {
                matches!(
                    error,
                    ActionProblem::Effect {
                        rule: EffectRule::Duration,
                        ..
                    }
                )
            })
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace(r#"kind = "physical" } }"#, r#"kind = "frost" } }"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, name, .. } if name == "frost"),
    ),
    Flaw::of(
        RIME,
        Edit::Replace(
            r#"id = "slow", duration_ms"#,
            r#"id = "frozen", duration_ms"#,
        ),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, name, .. } if name == "frozen"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#""projectiles", "#, ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Projectiles, at: Place::UnitType(name) } if name == "caster_creep_bolt"),
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"tower_bolt\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "recruit"),
                &ProductionDataError::Trained,
            )
        },
    },
    Flaw::of(
        "heroes/husk/scripts/extra.rhai",
        Edit::Create("fn on_resolve(ctx, caster, target) {}"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { path, problem: ScriptProblem::Unreferenced } if path.to_string() == "scripts/extra.rhai"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace("scripts/dread.rhai", "scripts/dreadful.rhai"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { path, problem: ScriptProblem::Missing } if path.to_string() == "scripts/dreadful.rhai"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("fn on_damage_taken(", "fn on_damage_takn("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_damage_takn"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("fn on_damage_taken(", "fn calc_damage_taken("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "calc_damage_taken"),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace(
            "fn on_think(ctx, unit) {",
            "fn on_resolve(ctx, caster, target) {}\n\nfn on_think(ctx, unit) {",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_resolve"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("fn on_resolve(", "fn on_cast("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_cast"),
    ),
    // A handle's param that no unit type or map marker holds.
    Flaw::of(
        CREEP_AI,
        Edit::Replace(
            "ctx.nearest_visible(unit, unit.params.aggro_range, \"enemies:creep\")",
            "ctx.nearest_visible(unit, unit.params.aggro_rang, \"enemies:creep\")",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, at: Place::Script(path), name } if path.as_str() == "scripts/creep_ai.rhai" && name == "aggro_rang"),
    ),
    // A handle's member every form of which is of a capability the mode does not declare: the
    // 3v3 has no `production`, whose `unit.load` it is.
    Flaw::of(
        CREEP_AI,
        Edit::Replace(
            "if target == () {\n        ctx.order_follow_path",
            "if unit.load > 0 {\n        ctx.order_follow_path",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::Script(path) } if path.as_str() == "scripts/creep_ai.rhai"),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace(
            "if target == () {\n        ctx.order_follow_path",
            "if unit.load > 0 {\n        ctx.order_follow_path",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::Script(path) } if path.as_str() == "scripts/creep_ai.rhai"),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace("fn on_think(ctx, unit)", "fn on_think(ctx, unit, more)"),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownHook(function), .. } if function == "on_think"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("    for unit in", "    let c = ctx;\n    for unit in"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::CtxMisuse(CtxMisuse::Stray),
                    ..
                }
            )
        },
    ),
    // A unit type's state field has a default of its type, and a script names after `.state`
    // only a field some state of the match declares.
    Flaw::of(
        CINDER,
        Edit::Set("units.chain_fire.state.bounces_left.default", r#""full""#),
        "hero-cinder",
        |problem| read_fails(problem, "data/avatar.toml", "a default not of the type"),
    ),
    Flaw::of(
        CHAIN_FIRE,
        Edit::Replace(
            "hit.delivery.state.bounces_left == 0",
            "hit.delivery.state.bounces == 0",
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownState(name), .. } if name == "bounces"),
    ),
    // A variable that nothing defines before its use fails the load, in a hook and in a
    // function of the script alike.
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            r#"ctx.damage(unit, "magic", ctx.params.damage);"#,
            r#"ctx.damage(unti, "magic", ctx.params.damage);"#,
        ),
        "hero-husk",
        |problem| undefined(problem, "unti"),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace(
            "let target = unit.attack_target;",
            "let target = unti.attack_target;",
        ),
        MODE,
        |problem| undefined(problem, "unti"),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace(
            "fn defend_hero(ctx, unit) {\n    for ally in ctx.find(",
            "fn defend_hero(c, unit) {\n    for ally in c.find(",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::CtxMisuse(CtxMisuse::Renamed { function }), .. } if function == "defend_hero"),
    ),
    // An engine enum's argument takes a member, never a string literal; a member and a function
    // are the enum's own; a module that is no engine enum's fails the compile.
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace("lane_end(ctx, team), wave", r#""start", wave"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::EnumString { call, takes: EngineEnum::PathEnd }, .. } if call == "spawn_group"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            "PathEnd::named(spawn_marker(ctx, team).params.from)",
            "PathEnd::Middle",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownEnumMember { path, of: EngineEnum::PathEnd }, .. } if path == "PathEnd::Middle"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace("PathEnd::named(spawn_marker", "PathEnd::name(spawn_marker"),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownEnumMember { path, of: EngineEnum::PathEnd }, .. } if path == "PathEnd::name"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace("PathEnd::named(spawn_marker", "Path::named(spawn_marker"),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(error), .. } if error.undefined() == Some(Undefined::Module("Path"))),
    ),
    // A literal compared with a name the API returns is a name of its kind, as the mode's damage
    // kinds `true` and `physical` are; an action's name compares through `is_action`, in its
    // package's scope; a `switch` on a name has cases no check reads.
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(r#"d.kind == "true""#, r#"d.kind == "pure""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, at: Place::Script(_), name } if name == "pure"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(r#"d.kind == "true""#, r#"d.action == "true""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::ScopedCompare(member), .. } if member == "action"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            r#"if d.kind == "true" {"#,
            r#"switch d.kind { "true" => {} }
    if d.kind == "true" {"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::NameSwitch(member), .. } if member == "kind"),
    ),
    // No script holds statements outside its functions, and none tries to catch a refusal.
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            "fn on_resolve(ctx, caster, target) {",
            "let radius = 3;\nfn on_resolve(ctx, caster, target) {",
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::TopLevel,
                    ..
                }
            )
        },
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            r#"        ctx.damage(unit, "magic", ctx.params.damage);"#,
            r#"        try { ctx.damage(unit, "magic", ctx.params.damage); } catch { }"#,
        ),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::Compile(_),
                    ..
                }
            )
        },
    ),
    // With no closures, a function that captures a variable fails to compile: it names a
    // variable its scope lacks.
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            r#"    for unit in ctx.find(caster, caster.position, ctx.params.radius, "enemies") {"#,
            r#"    let radius = ctx.params.radius;
    let near = |unit| unit.position.within(caster.position, radius);
    for unit in ctx.find(caster, caster.position, radius, "enemies") {"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Compile(error), .. } if error.undefined() == Some(Undefined::Variable("caster"))),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            "fn on_damage_taken(ctx, m, d)",
            "fn on_damage_taken(m, ctx, d)",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::CtxMisuse(CtxMisuse::HookParam { function }), .. } if function == "on_damage_taken"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("    for unit in", "    let ctx = 1;\n    for unit in"),
        "hero-husk",
        |problem| {
            matches!(
                problem,
                LoadProblem::Script {
                    problem: ScriptProblem::CtxMisuse(CtxMisuse::Bound),
                    ..
                }
            )
        },
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("caster.position", "caster.pos"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownMember(name), .. } if name == "pos"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            "    for unit in",
            "    ctx.spawn_avatars();\n    for unit in",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "spawn_avatars"),
    ),
    // A call or a method given a count of arguments no form of it takes, as `ctx.damage` takes
    // three and `distance_to` one.
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            "ctx.damage(unit, \"magic\", ctx.params.damage);",
            "ctx.damage(unit);",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Arity { name, args: 1 }, .. } if name == "ctx.damage"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            "    for unit in",
            "    let far = caster.position.distance_to();\n    for unit in",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Arity { name, args: 0 }, .. } if name == ".distance_to"),
    ),
];

#[test]
fn every_flaw_of_a_script_fails_its_load_with_its_own_problem() {
    fails_each(SCRIPT_FLAWS, |edits| load(&edited(edits.iter().copied())));
    // A name the check cannot follow, a variable its function binds by `let`, loads.
    let unfollowed = Edit::Replace(
        r#"    if name == "pick_end" {"#,
        r#"    let timer = name;
    if timer == "pick_ends" {}
    if name == "pick_end" {"#,
    );
    assert!(load(&edited([(MODE_SCRIPT, unfollowed)])).is_ok());
}
