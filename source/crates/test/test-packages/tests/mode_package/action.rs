//! Each flaw a package can have fails the load of the MOBA packages with its own problem.
use campfire_actions::{ActionField, ActionProblem, PipelineField};
use campfire_combat::CombatDataError;
use campfire_common::DeclaredName;
use campfire_mode::{ModeDataError, ModeError};
use campfire_navigation::{MapProblem, NavigationDataError};
use campfire_package::{LoadProblem, Place, ScriptProblem};
use campfire_projectiles::ProjectileError;
use campfire_script::Filter;
use campfire_test_packages::Edit;
use campfire_units::{StatId, TypesError};
use campfire_values::{ActionId, DamageKind, ModifierId, Named, Param};
use campfire_vision::VisionDataError;

use crate::flaws::{Flaw, fails_each};
use crate::moba::{edited, load};
use crate::module_rules::{entry, rule, rule_at, unit_type};

/// A flaw, the package it is in, and the problem it fails the load with.
use crate::mode_package::{
    CHAIN_FIRE, CREEP_AI, HUSK, HUSK_MANIFEST, LASH_OUT, MANIFEST, MAP, MODE, MODE_DATA,
    MODE_SCRIPT, RIME, UNITS, read_fails, stat_loop,
};

/// Flaws of a weapon, a place on the map, an AI, the visibility, the names the release plans,
/// a cooldown cut, a hold, a stat change and the loops of stats.
pub(super) static ACTION_FLAWS: &[Flaw] = &[
    // A weapon has a rate, a damage and a damage kind the mode declares, aims at a unit within
    // meters, and has no field only a cast runs; no other kind has a weapon's fields.
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "damage_kind = \"physical\"\ndelivery = { projectile = \"tower_bolt\" }",
            "damage_kind = \"fire\"\ndelivery = { projectile = \"tower_bolt\" }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, at: Place::Entry { table: "actions", name: action }, name: kind } if action == "tower_attack" && kind == "fire"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("rate = \"attack_speed\"\n", "rate = \"attack_sped\"\n"),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "melee_creep_attack" && name == "attack_sped"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("rate = \"attack_speed\"\n", ""),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a weapon's rate is a stat"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("actions.tower_attack.range", r#""global""#),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &ActionProblem::Global,
            )
        },
    ),
    // A minimum range lies within the range of meters, and is meters at least 0.
    Flaw::of(
        MODE_DATA,
        Edit::Set("actions.tower_attack.min_range", r#""999""#),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &ActionProblem::MinRange,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("actions.tower_attack.min_range", r#""-1""#),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &ActionProblem::Field(ActionField::MinRange),
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[actions.tower_attack]\nkind = \"attack\"\ntargeting = \"enemies\"",
            "[actions.tower_attack]\nkind = \"attack\"\ntargeting = \"point\"",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::AttackAims,
            )
        },
    ),
    // A rate bonus speeds a period of the weapon's own, and a streak's stages are some, their
    // shots strictly growing, each a modifier of its package
    // (04-capabilities/combat.md, A weapon's own period may speed up by a stat;
    // 04-capabilities/combat.md, A shot streak gives a stage's modifier).
    Flaw::of(
        MODE_DATA,
        Edit::Set("actions.tower_attack.rate_bonus", r#""attack_speed""#),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::RateBonus,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.streak",
            "{ coast_ms = 1000, stages = [] }",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::Stages,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.streak",
            r#"{ coast_ms = 1000, stages = [{ shots = 2, modifier = "warden_blessing" }, { shots = 2, modifier = "warden_blessing" }] }"#,
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::Stages,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.streak",
            r#"{ coast_ms = 1000, stages = [{ shots = 2, modifier = "spun_up" }] }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, name, .. } if name == "spun_up"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.streak",
            r#"{ coast_ms = 1000, stages = [{ shots = 2, modifier = "warden_blessing" }], fall = { modifier = "spun_down", ms = 1000 } }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, name, .. } if name == "spun_down"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[actions.wolf_attack]\n",
            "[actions.wolf_attack]\ncooldown_ms = 1000\n",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "wolf_attack"),
                &ActionProblem::KindField(PipelineField::CooldownMs),
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            "[actions.dread]\n",
            "[actions.dread]\nrate = \"attack_speed\"\n",
        ),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `rate`"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "damage_kinds = [\"physical\", \"magic\", \"true\"]",
            "damage_kinds = []",
        ),
        MODE,
        |problem| rule(problem, &CombatDataError::NoDamageKinds),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            r#"heal_scale = "healing_received_pct""#,
            r#"heal_scale = "heal_taken""#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Section("combat"), name } if name == "heal_taken"),
    ),
    Flaw::of(MAP, Edit::Remove("grid"), MODE, |problem| {
        rule(problem, &VisionDataError::NoGrid)
    }),
    Flaw::of(MAP, Edit::Remove("navigation"), MODE, |problem| {
        rule(problem, &NavigationDataError::NoPathingGrid)
    }),
    Flaw::of(MAP, Edit::Set("grid.cell", r#""0""#), MODE, |problem| {
        rule(problem, &VisionDataError::Grid)
    }),
    Flaw::of(
        MAP,
        Edit::Set("bounds", "{ min = [1, 0], max = [0, 1] }"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "map/two_lanes/map.toml",
                "bounds need min below max",
            )
        },
    ),
    Flaw::of(
        MAP,
        Edit::Set("units.0.position", "[0, -69]"),
        MODE,
        |problem| rule(problem, &ModeError::OutOfBounds),
    ),
    // A meter from the west lane's waypoint 1, at (−36, −36), less than the tower's body and the
    // widest walker's together.
    Flaw::of(
        MAP,
        Edit::Set("units.2.position", "[-36, -37]"),
        MODE,
        |problem| {
            let path = DeclaredName::new("west").unwrap();
            rule(problem, &MapProblem::WaypointBlocked { path, waypoint: 1 })
        },
    ),
    // A meter from the north spawn, at (0, −60).
    Flaw::of(
        MAP,
        Edit::Set("units.2.position", "[0, -59]"),
        MODE,
        |problem| {
            let marker = DeclaredName::new("north_spawn").unwrap();
            rule(problem, &MapProblem::MarkerBlocked { marker })
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Set("units.melee_creep.vision.sight_range", r#""-1.0""#),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/units.toml",
                "a sight range is a number that is not negative",
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Set("actions.lash_out.cost.mana", "-35"),
        "hero-husk",
        |problem| rule(problem, &ActionProblem::Field(ActionField::Cost)),
    ),
    Flaw {
        file: UNITS,
        edit: Edit::Replace("[units.melee_creep]", "[units.husk]\n\n[units.melee_creep]"),
        also: &[
            (MANIFEST, Edit::Replace("hero-husk = {", "husk = {")),
            (
                HUSK_MANIFEST,
                Edit::Replace(r#"name = "hero-husk""#, r#"name = "husk""#),
            ),
        ],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &Place::UnitTypes,
                &TypesError::Repeated("husk".to_owned()),
            )
        },
    },
    Flaw::of(
        LASH_OUT,
        Edit::Replace("ctx.find(", "ctx.finds("),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "finds"),
    ),
    // A call with `ctx` first is a call of `ctx`, as Rhai calls it, and a string literal index a
    // name of data, as a property is.
    Flaw::of(
        LASH_OUT,
        Edit::Replace("ctx.find(", "finds(ctx, "),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "finds"),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("ctx.params.radius", r#"ctx.params["radiuss"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, name, .. } if name == "radiuss"),
    ),
    // An AI's order in the mode's script.
    Flaw::of(
        "moba/modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    ctx.order_reset();",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::UnknownCtx(name), .. } if name == "order_reset"),
    ),
    // Taking a bot's slot is a late join.
    Flaw::of(
        MODE_DATA,
        Edit::Replace("late_join = true", "late_join = false"),
        MODE,
        |problem| rule(problem, &ModeDataError::BotTakeoverWithoutLateJoin),
    ),
    // An option whose default is none of its values; one of a param's name, which `ctx.params`
    // would read as one.
    Flaw::of(
        MODE_DATA,
        Edit::Set("options.income_gold", "{ values = [8, 16], default = 4 }"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "an option's default is one of its values",
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("options.income_gold", "{ values = [8, 16], default = 8 }"),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("options", "income_gold"),
                &ModeDataError::OptionParam,
            )
        },
    ),
    // `[visibility]`: a name no setting has; a recipient life's audience holds whatever the
    // mode says.
    Flaw::of(
        MODE_DATA,
        Edit::Set("visibility.fog", r#""all""#),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", r#""fog" is no setting"#),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("visibility.life", r#"["allies"]"#),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "no mode adds allies to the audience of life, or takes them from it",
            )
        },
    ),
    // The match has no owner or team for a mode field to audience to.
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "first_blood = { type = \"bool\", default = false, audience = \"all\" }",
            "first_blood = { type = \"bool\", default = false, audience = \"team\" }",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("state", "first_blood"),
                &ModeDataError::StateAudience,
            )
        },
    ),
    // Names the script API plans: a `ctx` method and field, a handle's field, a hook and data
    // fields.
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    ctx.generate(\"north\");",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "generate"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_match_start(ctx) {\n    let carried = ctx.carry;",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "carry"),
    ),
    Flaw::of(
        CHAIN_FIRE,
        Edit::Replace(
            "if hit.delivery.state",
            "if hit.part == () && hit.delivery.state",
        ),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "part"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            "fn on_match_start(ctx) {",
            "fn on_generate(ctx, region) {}\n\nfn on_match_start(ctx) {",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Script { problem: ScriptProblem::Planned(name), .. } if name == "on_generate"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]",
            "resources = [\"gold\"]\nstate_version = 1",
        ),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Planned {
                    field: "state_version",
                    at: Place::Mode
                }
            )
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Replace(
            "speed = \"12\", homing = true",
            "speed = \"12\", homing = true, gravity = \"9.8\"",
        ),
        MODE,
        |problem| rule_at(problem, &unit_type("tower_bolt"), &ProjectileError::Gravity),
    ),
    Flaw::of(
        LASH_OUT,
        Edit::Replace("ctx.params.radius", "ctx.params.radios"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "radios"),
    ),
    // A cooldown cut names an ability of the script's own package.
    Flaw::of(
        LASH_OUT,
        Edit::Replace(
            r#"ctx.cut_cooldown(m.carrier, "lash_out""#,
            r#"ctx.cut_cooldown(m.carrier, "lash""#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: ActionId::NAMES, at: Place::Script(path), name } if path.to_string() == "scripts/lash_out.rhai" && name == "lash"),
    ),
    Flaw::of(
        "moba/heroes/cinder/scripts/wildfire.rhai",
        Edit::Replace(r#""kindle""#, r#""kindl""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, name: id, .. } if id == "kindl"),
    ),
    Flaw::of(
        "moba/heroes/veil/scripts/dual_path.rhai",
        Edit::Replace(r#"stat("attack_damage")"#, r#"stat("attack_dmg")"#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "attack_dmg"),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace("enemies:creep", "enemies:minion"),
        "test-ai",
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "enemies:minion"),
    ),
    Flaw::of(
        HUSK,
        Edit::Rename("stats.magic_resist", "spirit"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "spirit"),
    ),
    Flaw::of(
        HUSK,
        Edit::Rename("modifiers.withered.stats.magic_resist", "spirit"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "spirit"),
    ),
    // A hold needs a toggle or a channel to hold its modifier while.
    Flaw::of(
        RIME,
        Edit::Remove("actions.chill_arrows.toggle"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "chill_arrows"),
                &ActionProblem::HoldAlone,
            )
        },
    ),
    // Only a point aim clamps to the range.
    Flaw::of(
        "moba/spells/data/loadout.toml",
        Edit::Set("actions.blink.targeting", r#""direction""#),
        "player-spells",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "blink"),
                &ActionProblem::ClampAims,
            )
        },
    ),
    // A stat change names one operation; a scaling param scales with declared stats alone.
    Flaw::of(
        "moba/spells/data/loadout.toml",
        Edit::Set("modifiers.haste.stats.move_speed.cut", r#""0.1""#),
        "player-spells",
        |problem| {
            read_fails(
                problem,
                "data/loadout.toml",
                "a stat change is a number, or one of",
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Rename(
            "actions.grasping_wraps.params.damage.ability_power",
            "spell_power",
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "actions", name: _ }, name } if name == "spell_power"),
    ),
    // The mode's `surge` adds attack damage by a param that only the mode's action `surge`,
    // which applies it, declares, and that reads spell vamp; `vamp` adds spell vamp by its own
    // param, which reads attack damage: a loop that only the mode's actions show.
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[actions.melee_creep_attack]",
            "[modifiers.surge]\nstats = { attack_damage = { param = \"surge\" } }\n\n[modifiers.vamp]\nstats = { spell_vamp = { param = \"vamp\" } }\n[modifiers.vamp.params]\nvamp = { base = 0, attack_damage = \"1\" }\n\n[actions.surge]\ntargeting = \"none\"\npassive_modifier = \"surge\"\n[actions.surge.params]\nsurge = { base = 0, spell_vamp = \"10\" }\n\n[actions.melee_creep_attack]",
        ),
        MODE,
        |problem| stat_loop(problem, ["attack_damage", "spell_vamp"]),
    ),
    // Dual Path's spell vamp reads attack damage; a modifier whose attack damage reads spell
    // vamp closes a loop.
    Flaw::of(
        "moba/heroes/veil/data/avatar.toml",
        Edit::Replace(
            "[modifiers.dual_path.params]",
            "[modifiers.loop]\nstats = { attack_damage = { param = \"thirst\" } }\n[modifiers.loop.params]\nthirst = { base = 0, spell_vamp = \"10\" }\n\n[modifiers.dual_path.params]",
        ),
        MODE,
        |problem| stat_loop(problem, ["attack_damage", "spell_vamp"]),
    ),
    Flaw::of(
        "moba/heroes/veil/data/avatar.toml",
        Edit::Replace("bonus = { attack_damage =", "bonus = { attack_dmg ="),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "modifiers", name: id }, name } if id == "dual_path" && name == "attack_dmg"),
    ),
];

#[test]
fn every_flaw_of_an_action_or_a_loop_of_stats_fails_its_load_with_its_own_problem() {
    fails_each(ACTION_FLAWS, |edits| load(&edited(edits.iter().copied())));
}
