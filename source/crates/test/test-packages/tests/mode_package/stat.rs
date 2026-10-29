//! Each flaw a package can have fails the load of the MOBA packages with its own problem.
use campfire_actions::{ActionProblem, CostTarget, SlotError};
use campfire_combat::{Armor, CombatDataError};
use campfire_common::DeclaredName;
use campfire_declarations::{BookPlace, NamesListError, TypePlace};
use campfire_mode::ModeError;
use campfire_orders::AiError;
use campfire_package::{LoadProblem, LocaleProblem, Place};
use campfire_script::Filter;
use campfire_script::ParamProblem;
use campfire_stats::{AppliedError, ModifierDataError, ModifierProblem, UnitStartError, Way};
use campfire_test_packages::Edit;
use campfire_units::{PoolId, StatId, UnitTypeError};
use campfire_values::{
    ActionId, DamageKind, ModifierId, Named, Param, Tag, Team, TextId, TimeTooLarge, TrackId,
    UnitType,
};

use crate::flaws::{Flaw, fails_each};
use crate::moba::{edited, load};
use crate::module_rules::{entry, rule, rule_at, rule_of, unit_type};

/// A flaw, the package it is in, and the problem it fails the load with.
use crate::mode_package::{
    CINDER, CREEP_AI, GALE, HUSK, HUSK_MANIFEST, HUSK_TEXT, MANIFEST, MAP, MODE, MODE_DATA,
    MODE_DIR, MODE_SCRIPT, MODE_TEXT, RIME, STILLNESS, UNITS, action_place, by, manifest_fails,
    name, read_fails, shared_passive, slow_fails,
};

/// Flaws of a pool, a match's start, a stat change, a modifier's owner, an aura, a way's params,
/// a name a script gives, a text and an engine stat.
pub(super) static STAT_FLAWS: &[Flaw] = &[
    // Pools: each a unit lists, a cost names or a script reads is declared, and the life pool is
    // among a combatant's; no pool shares a name with a player resource, and no more than eight.
    Flaw::of(
        HUSK,
        Edit::Replace(
            r#"pools = ["health", "mana"]"#,
            r#"pools = ["health", "rage"]"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: PoolId::NAMES, at: Place::Avatar(_), name } if name == "rage"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            r#"pools = ["health", "mana"]"#,
            r#"pools = ["health", "mana", "mana"]"#,
        ),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &NamesListError| {
                let mana = NamesListError::Repeated(DeclaredName::new("mana").unwrap());
                matches!(at, Place::Avatar(_)) && *error == mana
            })
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#"pools = ["health", "mana"]"#, r#"pools = ["mana"]"#),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &UnitStartError| {
                matches!(at, Place::Avatar(_)) && *error == UnitStartError::NoLifePool
            })
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Replace("pools = [\"health\"]\n", ""),
        MODE,
        |problem| {
            rule_at(
                problem,
                &unit_type("melee_creep"),
                &UnitStartError::NoLifePool,
            )
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Replace("combat = { on_death = \"stay\" }\n", ""),
        MODE,
        |problem| rule_at(problem, &unit_type("inhibitor"), &UnitStartError::NoCombat),
    ),
    Flaw::of(
        UNITS,
        Edit::Replace(r#""tower", "true_sight"]"#, r#""tower", "avatar"]"#),
        MODE,
        |problem| {
            let avatar = UnitTypeError::EngineTag(DeclaredName::new("avatar").unwrap());
            rule_at(problem, &unit_type("tower"), &avatar)
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#"tags = ["tank"]"#, r#"tags = ["area"]"#),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &UnitTypeError| {
                let area = UnitTypeError::EngineTag(DeclaredName::new("area").unwrap());
                matches!(at, Place::Avatar(_)) && *error == area
            })
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace(r#"tags = ["slowed"]"#, r#"tags = ["projectile"]"#),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "slow"),
                &ModifierDataError::EngineTag(name("projectile")),
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#"tags = ["tank"]"#, r#"tags = ["tank:front"]"#),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", r#""tank:front" is not a name"#),
    ),
    Flaw::of(
        "moba/heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"left = { type = "int""#, r#"Left = { type = "int""#),
        "hero-kensho",
        |problem| read_fails(problem, "data/avatar.toml", r#""Left" is not a name"#),
    ),
    // What a match start refused before, now at load: a unit type that makes no unit, an AI
    // that does not load, and a time that does not count in ticks at the fastest rate, 60 Hz.
    // 4 × 10¹⁷ ms counts at the default 30 Hz, 1.2 × 10¹⁹ ticks, and not at 60 Hz, past 2⁶⁴.
    Flaw::of(
        UNITS,
        Edit::Set("units.melee_creep.stats.health.base", "0"),
        MODE,
        |problem| {
            rule_of(problem, |at, error: &UnitStartError| {
                *at == unit_type("melee_creep") && matches!(error, UnitStartError::NotPositive(_))
            })
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Set("units.melee_creep.orders.think_ms", "400000000000000000"),
        MODE,
        |problem| rule(problem, &AiError::Time(TimeTooLarge)),
    ),
    Flaw::of(
        CREEP_AI,
        Edit::Replace("fn on_think(ctx, unit) {", "fn think(ctx, unit) {"),
        MODE,
        |problem| {
            rule_of(problem, |at, error: &AiError| {
                matches!(at, Place::UnitType(_)) && *error == AiError::NoThink
            })
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Set("actions.lash_out.cooldown_ms.4", "400000000000000000"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "lash_out"),
                &ActionProblem::Time(TimeTooLarge),
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Set("modifiers.withered.duration_ms", "-1"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "withered"),
                &ModifierProblem::Time,
            )
        },
    ),
    // A stat change past what a number holds: 9 × 10¹² is past 2³⁹, as a number has 24 bits
    // of fraction in 64.
    Flaw::of(
        HUSK,
        Edit::Set("modifiers.withered.stats.magic_resist", "-9000000000000"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "withered"),
                &ModifierProblem::Overflow,
            )
        },
    ),
    Flaw::of(
        CINDER,
        Edit::Set("units.eruption.area.delay_ms", "400000000000000000"),
        "hero-cinder",
        |problem| rule_at(problem, &unit_type("eruption"), &TimeTooLarge),
    ),
    // An avatar package named as its own delivery type: the area's time names the delivery type.
    Flaw {
        file: CINDER,
        edit: Edit::Set("units.eruption.area.delay_ms", "400000000000000000"),
        also: &[
            (MANIFEST, Edit::Replace("hero-cinder = {", "eruption = {")),
            (
                "moba/heroes/cinder/manifest.toml",
                Edit::Replace(r#"name = "hero-cinder""#, r#"name = "eruption""#),
            ),
        ],
        package: "eruption",
        refused: |problem| rule_at(problem, &unit_type("eruption"), &TimeTooLarge),
    },
    // A modifier is the passive of one owner of its package: Husk's avatar holds
    // `withering_touch`, and Lash Out `lash_out_guard`.
    Flaw::of(
        HUSK,
        Edit::Set("actions.lash_out.passive_modifier", r#""withering_touch""#),
        "hero-husk",
        |problem| {
            shared_passive(
                problem,
                "withering_touch",
                BookPlace::UnitType(TypePlace::Avatar),
                "lash_out",
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Set("actions.dread.passive_modifier", r#""lash_out_guard""#),
        "hero-husk",
        |problem| shared_passive(problem, "lash_out_guard", action_place("dread"), "lash_out"),
    ),
    // The same of the mode, a unit type's and an action's, and of a loadout, two actions'.
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.tower.passive", r#""warden_blessing""#),
        also: &[(
            MODE_DATA,
            Edit::Set(
                "actions.tower_attack.passive_modifier",
                r#""warden_blessing""#,
            ),
        )],
        package: MODE,
        refused: |problem| {
            shared_passive(
                problem,
                "warden_blessing",
                BookPlace::UnitType(TypePlace::Declared(name("tower"))),
                "tower_attack",
            )
        },
    },
    Flaw {
        file: "moba/spells/data/loadout.toml",
        edit: Edit::Set("actions.blink.passive_modifier", r#""haste""#),
        also: &[(
            "moba/spells/data/loadout.toml",
            Edit::Set("actions.haste.passive_modifier", r#""haste""#),
        )],
        package: "player-spells",
        refused: |problem| shared_passive(problem, "haste", action_place("blink"), "haste"),
    },
    // A modifier grants no engine tag: one that granted `avatar` would split who a player
    // commands between the type's own tags and the unit's.
    Flaw::of(
        GALE,
        Edit::Set("modifiers.fair_wind.tags", r#"["avatar"]"#),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "fair_wind"),
                &ModifierDataError::EngineTag(name("avatar")),
            )
        },
    ),
    // A projectile type holds no passive, as a delivery's spawn gives none: a type with one is
    // a type that stands, which no delivery names.
    Flaw::of(
        GALE,
        Edit::Set("units.wind_bolt.passive", r#""wind_shield""#),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &unit_type("wind_bolt"),
                &UnitTypeError::NotDelivery,
            )
        },
    ),
    // An aura's radius and a shield are never negative: as Gale's Fair Wind's value, as a rank
    // of the Wind Shield action's param its modifier reads, or as the modifier's own param.
    Flaw::of(
        GALE,
        Edit::Set("modifiers.fair_wind.aura.radius", r#""-1.0""#),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "fair_wind"),
                &AppliedError::Sign(ModifierProblem::Negative),
            )
        },
    ),
    Flaw::of(
        GALE,
        Edit::Set(
            "actions.wind_shield.params.shield.base",
            "[80, 120, -1, 200, 240]",
        ),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "wind_shield"),
                &AppliedError::Sign(ModifierProblem::Negative),
            )
        },
    ),
    Flaw::of(
        GALE,
        Edit::Set("modifiers.wind_shield.params.shield", "-5"),
        "hero-gale",
        |problem| {
            rule_at(
                problem,
                &entry("modifiers", "wind_shield"),
                &AppliedError::Sign(ModifierProblem::Negative),
            )
        },
    ),
    // A way that applies a modifier gives each param it reads and does not declare: Rime's Slow
    // reads `slow` of Fan of Frost, which applies it on hit; of Chill Arrows, whose held
    // modifier's script applies it; of Snow Owl, whose passive's aura applies it; and of no
    // action, with which her own passive's script applies it.
    Flaw::of(
        RIME,
        Edit::Remove("actions.fan_of_frost.params.slow"),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("fan_of_frost")), ParamProblem::Missing),
    ),
    Flaw::of(
        RIME,
        Edit::Remove("actions.chill_arrows.params.slow"),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("chill_arrows")), ParamProblem::Missing),
    ),
    Flaw::of(
        RIME,
        Edit::Set(
            "modifiers.snow_owl_bounty.aura",
            r#"{ radius = "1.0", affects = "enemies", modifier = "slow" }"#,
        ),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("snow_owl")), ParamProblem::Missing),
    ),
    Flaw::of(
        STILLNESS,
        Edit::Replace(
            "m.stacks += 1;",
            "m.stacks += 1;\n    ctx.add_modifier(m.carrier, \"slow\");",
        ),
        "hero-rime",
        |problem| slow_fails(problem, Some(&Way::NoAction), ParamProblem::Missing),
    ),
    // Its own per-rank param has a value at each rank of each action that applies it, and a
    // time reads no scaling param, whose value only the source knows as it applies.
    Flaw::of(
        RIME,
        Edit::Set("modifiers.slow.params.slow", r#"["0.1", "0.2", "0.3"]"#),
        "hero-rime",
        |problem| slow_fails(problem, Some(&by("chill_arrows")), ParamProblem::Short),
    ),
    Flaw {
        file: RIME,
        edit: Edit::Set("modifiers.stun.duration_ms", r#"{ param = "stun_ms" }"#),
        also: &[(
            RIME,
            Edit::Set(
                "modifiers.stun.params.stun_ms",
                r#"{ base = 1000, ability_power = "1.0" }"#,
            ),
        )],
        package: "hero-rime",
        refused: |problem| {
            rule_at(
                problem,
                &entry("modifiers", "stun"),
                &AppliedError::Param {
                    param: name("stun_ms"),
                    way: None,
                    problem: ParamProblem::ScalingTime,
                },
            )
        },
    },
    // A name a script gives the API, as the registry marks the argument: a tag, a track, a unit
    // type of the mode's scope, a team.
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(r#"unit.has_tag("core")"#, r#"unit.has_tag("cor")"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Tag::NAMES, name, .. } if name == "cor"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            r#"ctx.all_units_tagged("inhibitor")"#,
            r#"ctx.all_units_tagged("inhibitors")"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Tag::NAMES, name, .. } if name == "inhibitors"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(r#"track = "level""#, r#"track = "levels""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TrackId::NAMES, name, .. } if name == "levels"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            "    if unit.has_tag(\"inhibitor\") {\n",
            "    ctx.spawn_unit(\"grasping_wraps\", \"north\", unit.position);\n    if unit.has_tag(\"inhibitor\") {\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "grasping_wraps"),
    ),
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(
            "    if unit.has_tag(\"inhibitor\") {\n",
            "    ctx.spawn_unit(\"hero-husk\", \"west\", unit.position);\n    if unit.has_tag(\"inhibitor\") {\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Team::NAMES, name, .. } if name == "west"),
    ),
    // Human text: each message an avatar names has a value in its own language's file, and each
    // file under `locale/` is `<language>.ftl`, parses, defines no message twice, and in another
    // language defines only the own file's messages.
    Flaw::of(
        HUSK,
        Edit::Replace(r#"name = "hero-name""#, r#"name = "hero-nam""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: TextId::NAMES, at: Place::Avatar(package), name } if package == "hero-husk" && name == "hero-nam"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#"name = "hero-name""#, r#"name = "hero name""#),
        "hero-husk",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                r#""hero name" is not a text id"#,
            )
        },
    ),
    Flaw::of(
        HUSK_TEXT,
        Edit::Replace("hero-name = Husk", "hero-name =\n    .short = Hu"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: TextId::NAMES, name, .. } if name == "hero-name"),
    ),
    // The names players see: the mode's, each team's, each choice's and each of an avatar's or a
    // loadout's actions, each a message of its package's own language.
    Flaw::of(
        MODE_TEXT,
        Edit::Replace("mode-name = Three on Three\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TextId::NAMES, at: Place::Mode, name } if name == "mode-name"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("[teams.camps]\nname = \"team-camps\"\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::UnnamedTeam(team) if team == "camps"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[teams.north]",
            "[teams.river]\nname = \"team-north\"\n\n[teams.north]",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Team::NAMES, at: Place::Mode, name } if name == "river"),
    ),
    Flaw::of(
        MODE_TEXT,
        Edit::Replace("choice-hero = Hero\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TextId::NAMES, at: Place::Choice(choice), name } if choice == "hero" && name == "choice-hero"),
    ),
    Flaw::of(
        HUSK,
        Edit::Remove("actions.dread.name"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::UnnamedAction(action) if action == "dread"),
    ),
    Flaw::of(
        "moba/spells/data/loadout.toml",
        Edit::Remove("actions.blink.name"),
        "player-spells",
        |problem| matches!(problem, LoadProblem::UnnamedAction(action) if action == "blink"),
    ),
    Flaw::of(
        HUSK_TEXT,
        Edit::Replace("action-dread = Dread\n", ""),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: TextId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "dread" && name == "action-dread"),
    ),
    Flaw::of(
        HUSK_TEXT,
        Edit::Replace("hero-name = Husk", "hero-name Husk"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { path, problem: LocaleProblem::Parse(_) } if path.as_str() == "locale/en.ftl"),
    ),
    Flaw::of(
        HUSK_TEXT,
        Edit::Replace("hero-name = Husk", "hero-name = Husk\nhero-name = Hulk"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { problem: LocaleProblem::Repeated(id), .. } if id.as_str() == "hero-name"),
    ),
    Flaw::of(
        "moba/heroes/husk/locale/de.ftl",
        Edit::Create("hero-nme = Hülse\n"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { path, problem: LocaleProblem::Stray(id) } if path.as_str() == "locale/de.ftl" && id.as_str() == "hero-nme"),
    ),
    Flaw::of(
        "moba/heroes/husk/locale/de_DE.ftl",
        Edit::Create("hero-name = Hülse\n"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Locale { path, problem: LocaleProblem::FileName } if path.as_str() == "locale/de_DE.ftl"),
    ),
    Flaw::of(
        HUSK_MANIFEST,
        Edit::Replace(r#"language = "en""#, r#"language = "EN""#),
        "hero-husk",
        |problem| manifest_fails(problem, r#""EN" is not a language identifier"#),
    ),
    // 2⁴⁰ is past the 2³⁹ a number holds.
    Flaw::of(
        HUSK,
        Edit::Set("stats.health.base", "1099511627776"),
        "hero-husk",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "1099511627776 is past what a number holds",
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Set("stats.health.per_level", "1099511627776"),
        "hero-husk",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "1099511627776 is past what a number holds",
            )
        },
    ),
    Flaw::of(
        "moba/heroes/veil/data/avatar.toml",
        Edit::Set(
            "modifiers.dual_path.params.spell_vamp.bonus.attack_damage",
            "1099511627776",
        ),
        "hero-veil",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "data did not match any variant of untagged enum Param",
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace("[actions.lash_out]", "[actions.lash-out]"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", r#""lash-out" is not a name"#),
    ),
    Flaw::of(
        RIME,
        Edit::Replace(r#"{ param = "slow_ms" }"#, r#"{ param = "slow ms" }"#),
        "hero-rime",
        |problem| {
            read_fails(
                problem,
                "data/avatar.toml",
                "data did not match any variant of untagged enum Number",
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace(r#"hits = "enemies:avatar""#, r#"hits = "enemies:Avatar""#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", r#"filter "enemies:Avatar""#),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#"{ name = "north""#, r#"{ name = "North""#),
        MODE_DIR,
        |problem| manifest_fails(problem, r#""North" is not a name"#),
    ),
    Flaw::of(
        MAP,
        Edit::Replace(r#"path = "west""#, r#"path = "west lane""#),
        MODE,
        |problem| {
            read_fails(
                problem,
                "map/two_lanes/map.toml",
                r#""west lane" is not a name"#,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Rename("params.income_ms", "income-ms"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", r#""income-ms" is not a name"#),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("[tags.stunned]", "[tags.Stunned]"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", r#""Stunned" is not a name"#),
    ),
    Flaw::of(
        HUSK,
        Edit::Rename("actions.lash_out.cost.mana", "rage"),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: CostTarget::NAMES, at: Place::Entry { table: "actions", name: id }, name } if id == "lash_out" && name == "rage"),
    ),
    Flaw::of(
        "moba/heroes/veil/scripts/dusk_mark.rhai",
        Edit::Replace(r#""energy""#, r#""rage""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: PoolId::NAMES, at: Place::Script(_), name } if name == "rage"),
    ),
    Flaw::of(
        "moba/heroes/veil/scripts/dusk_mark.rhai",
        Edit::Replace(r#"is_action("dusk_mark")"#, r#"is_action("dusk")"#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: ActionId::NAMES, at: Place::Script(_), name } if name == "dusk"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("life = \"health\"\n", ""),
        MODE,
        |problem| rule(problem, &CombatDataError::NoLifePool),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(r#"life = "health""#, r#"life = "hp""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: PoolId::NAMES, at: Place::Section("combat"), name } if name == "hp"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("[pools.mana]\nmax = \"mana\"", "[pools.mana]\nmax = \"mp\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, at: Place::Entry { table: "pools", name: pool }, name } if pool.as_str() == "mana" && name == "mp"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[pools.energy]",
            "[pools.p1]\nmax = \"mana\"\n[pools.p2]\nmax = \"mana\"\n[pools.p3]\nmax = \"mana\"\n[pools.p4]\nmax = \"mana\"\n[pools.p5]\nmax = \"mana\"\n[pools.p6]\nmax = \"mana\"\n[pools.energy]",
        ),
        MODE,
        |problem| rule(problem, &NamesListError::TooMany),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "gold"]"#),
        MODE,
        |problem| {
            let gold = NamesListError::Repeated(DeclaredName::new("gold").unwrap());
            rule_at(problem, &Place::Section("resources"), &gold)
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(r#"resources = ["gold"]"#, r#"resources = ["gold", "mana"]"#),
        MODE,
        |problem| {
            let mana = NamesListError::Repeated(DeclaredName::new("mana").unwrap());
            rule_at(problem, &Place::Section("resources"), &mana)
        },
    ),
    // An engine stat is declared like any other: one the units carry and the mode does not
    // declare fails.
    Flaw::of(
        MODE_DATA,
        Edit::Replace("move_speed = {}\n", ""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "move_speed"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("stats.cooldown_reduction", r#"{ min = 1, max = "0.4" }"#),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a stat's min passes its max"),
    ),
    // An armor of a damage kind the mode does not declare, or a unit type of an armor the mode
    // does not declare, fails.
    Flaw::of(
        MODE_DATA,
        Edit::Set("combat.armors.plate", r#"{ cold = "0.5" }"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, name, .. } if name == "cold"),
    ),
    Flaw::of(
        UNITS,
        Edit::Set("units.inhibitor.combat.armor", r#""plate""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Armor::NAMES, name, .. } if name == "plate"),
    ),
    Flaw::of(
        "moba/heroes/veil/scripts/whirling_blades.rhai",
        Edit::Replace(r#""physical""#, r#""fire""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: DamageKind::NAMES, name: kind, .. } if kind == "fire"),
    ),
    Flaw::of(
        GALE,
        Edit::Replace(
            r#"{ param = "bonus_speed" }"#,
            r#"{ param = "bonus_speeds" }"#,
        ),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Unknown { of: Param::NAMES, at: Place::Entry { table: "modifiers", name: id }, name } if id == "gust_speed" && name == "bonus_speeds"),
    ),
    Flaw::of(
        "moba/heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"hold = "still_mind""#, r#"hold = "still_mindful""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::Entry { table: "actions", name: ability }, name: id } if ability == "still_mind" && id == "still_mindful"),
    ),
    Flaw::of(
        GALE,
        Edit::Replace(r#"affects = "allies""#, r#"affects = "allies:friends""#),
        "hero-gale",
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "allies:friends"),
    ),
    Flaw::of(
        "moba/heroes/rime/data/avatar.toml",
        Edit::Replace(r#"hits = "enemies:avatar""#, r#"hits = "foes:avatar""#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", "filter \"foes:avatar\""),
    ),
    Flaw::of(
        "moba/heroes/kensho/data/avatar.toml",
        Edit::Set("modifiers.flicker_strike.state.left.audience", r#""all""#),
        "hero-kensho",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `audience`"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#""dread", "lash_out""#, r#""grasping_wraps", "lash_out""#),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &SlotError| {
                let twice = SlotError::Repeated(DeclaredName::new("grasping_wraps").unwrap());
                matches!(at, Place::Avatar(_)) && *error == twice
            })
        },
    ),
    Flaw::of(
        MAP,
        Edit::Replace(r#"unit_type = "core""#, r#"unit_type = "cor""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownUnitType(DeclaredName::new("cor").unwrap()),
            )
        },
    ),
    Flaw::of(
        MAP,
        Edit::Replace(r#"path = "west""#, r#"path = "north""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownPath(DeclaredName::new("north").unwrap()),
            )
        },
    ),
];

#[test]
fn every_flaw_of_a_stat_a_modifier_or_a_text_fails_its_load_with_its_own_problem() {
    fails_each(STAT_FLAWS, |edits| load(&edited(edits.iter().copied())));
    // `has_modifier` only names a modifier: Rime's passive's script may ask for the Slow it
    // gives no param of, where an application of it is a flaw.
    let asks = Edit::Replace(
        "m.stacks += 1;",
        r#"if m.carrier.has_modifier("slow") { m.stacks += 1; }"#,
    );
    assert!(load(&edited([(STILLNESS, asks)])).is_ok());
}
