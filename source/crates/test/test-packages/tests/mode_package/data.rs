//! Each flaw a package can have fails the load of the MOBA packages with its own problem.

use crate::flaws::{Flaw, fails_each};
use campfire_actions::{ActionProblem, EffectRule, ListHook, PlannedEffect};
use campfire_areas::AreaDeliveryError;
use campfire_capabilities::CapabilityError;
use campfire_combat::CombatDataError;
use campfire_common::DeclaredName;
use campfire_mode::ModeError;
use campfire_orders::AiError;
use campfire_package::{LoadProblem, Place};
use campfire_progression::TrackError;
use campfire_projectiles::{ProjectileDeliveryError, ProjectileError};
use campfire_script::Filter;
use campfire_sim::Capability;
use campfire_test_packages::Edit;
use campfire_units::{StatId, UnitTypeError};
use campfire_values::{ActionId, ModifierId, Named, Tag, TrackId, UnitType};

use crate::moba::{edited, load};
use crate::module_rules::{entry, rule, rule_at, rule_of, unit_type};

/// A flaw, the package it is in, and the problem it fails the load with.
use crate::mode_package::{
    CINDER, HUSK, MANIFEST, MAP, MODE, MODE_DATA, MODE_DIR, RIME, SLOWS, SNOW_OWL, UNITS, VEIL,
    manifest_fails, read_fails,
};

/// Flaws of the manifest, the data's sections, the tracks, the slot kinds' levels, the call
/// budget, and an action's delivery, spawn and move: each to a copy of the packages, and the
/// problem it fails the load with.
pub(super) static DATA_FLAWS: &[Flaw] = &[
    // The release runs package API 1.0: another major, and a newer minor, do not load.
    Flaw::of(
        MANIFEST,
        Edit::Set("api", r#""2.0""#),
        MODE,
        |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "2.0"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Set("api", r#""1.0.0""#),
        MODE_DIR,
        |problem| manifest_fails(problem, r#""1.0.0" is not major.minor"#),
    ),
    Flaw::of(
        "heroes/husk/manifest.toml",
        Edit::Set("api", r#""1.1""#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "1.1"),
    ),
    // A newer package's manifest may hold a field this release does not know: its version is
    // read first, so it fails as `OtherApi`, the mode's and a dependency's alike.
    Flaw {
        file: MANIFEST,
        edit: Edit::Set("api", r#""1.1""#),
        also: &[(MANIFEST, Edit::Set("future", "true"))],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "1.1"),
    },
    Flaw {
        file: "heroes/husk/manifest.toml",
        edit: Edit::Set("api", r#""2.0""#),
        also: &[("heroes/husk/manifest.toml", Edit::Set("future", "true"))],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::OtherApi(api) if api.to_string() == "2.0"),
    },
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#"["combat","#, r#"["mode", "combat","#),
        MODE_DIR,
        |problem| {
            matches!(
                problem,
                LoadProblem::Capabilities(CapabilityError::DeclaresMode)
            )
        },
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#"["combat","#, r#"["combat", "combat","#),
        MODE_DIR,
        |problem| {
            matches!(
                problem,
                LoadProblem::Capabilities(CapabilityError::Repeated(Capability::Combat))
            )
        },
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#", "navigation""#, ""),
        MODE_DIR,
        |problem| {
            matches!(
                problem,
                LoadProblem::Capabilities(CapabilityError::Needs {
                    capability: Capability::Orders,
                    needs: Capability::Navigation,
                })
            )
        },
    ),
    // A section of the mode's data or of its map is its capability's, declared or refused: the
    // map's vision grid without `vision`, the mode's `[navigation]` without `navigation`,
    // `[supply]` without `production`.
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#", "vision", "progression""#, r#", "progression""#),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Vision,
                    at: Place::MapSection("grid")
                }
            )
        },
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#", "orders", "navigation""#, ""),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Navigation,
                    at: Place::Section("navigation")
                }
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("supply", "{ max = 10 }"),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Production,
                    at: Place::Section("supply")
                }
            )
        },
    ),
    // Tracks and a death's experience are progression's: tracks positive and ascending, at most
    // one the `level` track, and each a unit type lists one the mode declares; the experience
    // section the first the load finds.
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#", "progression", "items"]"#, r#", "items"]"#),
        MODE,
        |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Progression,
                    at: Place::Section("experience")
                }
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set("tracks.level.levels", "[660, 280]"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a track has a level 2"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[tracks.level]\n",
            "[tracks.fame]\nlevel = true\nlevels = [10]\n\n[tracks.level]\n",
        ),
        MODE,
        |problem| rule(problem, &TrackError::LevelTracks),
    ),
    // A slot kind's levels: one for each of its ranks, each at least 1 and at least the one
    // before, on a kind with ranks, in a mode with the `level` track.
    Flaw::of(
        MODE_DATA,
        Edit::Replace("levels = [6, 11, 16]", "levels = [6, 11]"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "a slot kind's `levels` gives one level",
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("levels = [6, 11, 16]", "levels = [6, 16, 11]"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "a slot kind's `levels` gives one level",
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("levels = [1, 3, 5, 7, 9]", "levels = [0, 3, 5, 7, 9]"),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "a level is at least 1"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("name = \"spell\"\n", "name = \"spell\"\nlevels = [1]\n"),
        MODE,
        |problem| {
            read_fails(
                problem,
                "data/mode.toml",
                "a slot kind's `levels` needs its `ranks`",
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("level = true\n", ""),
        MODE,
        |problem| {
            rule(
                problem,
                &TrackError::RankLevels(DeclaredName::new("basic").unwrap()),
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#"tracks = ["level"]"#, r#"tracks = ["levels"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: TrackId::NAMES, name, .. } if name == "levels"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Set("tick_hz", "{ min = 40, max = 60, default = 30 }"),
        MODE_DIR,
        |problem| manifest_fails(problem, "the tick rate range does not hold its default"),
    ),
    // A call of no operation, which Rhai would read as no limit.
    Flaw::of(
        MANIFEST,
        Edit::Set(
            "script_limits",
            "{ per_call = 0, player = 40000, think = 200000, mode = 100000 }",
        ),
        MODE_DIR,
        |problem| {
            manifest_fails(
                problem,
                "invalid value: integer `0`, expected a nonzero u64",
            )
        },
    ),
    // A player's pool below the whole call of 20 000.
    Flaw::of(
        MANIFEST,
        Edit::Set(
            "script_limits",
            "{ per_call = 20000, player = 19999, think = 200000, mode = 100000 }",
        ),
        MODE_DIR,
        |problem| manifest_fails(problem, "a script pool holds less than a whole call"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Set(
            "script_limits",
            "{ per_call = 20000, player = 40000, think = 200000, mode = 19999 }",
        ),
        MODE_DIR,
        |problem| manifest_fails(problem, "a script pool holds less than a whole call"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Set("max_move_speed", r#""0""#),
        MODE_DIR,
        |problem| manifest_fails(problem, "a speed is a positive number"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace("hero-husk = {", "hero-hask = {"),
        "hero-hask",
        |problem| matches!(problem, LoadProblem::OtherName(name) if name == "hero-husk"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(r#""tomb_bind"]"#, r#""tomb_binds"]"#),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: ActionId::NAMES, at: Place::Avatar(_), name } if name == "tomb_binds"),
    ),
    Flaw::of(
        HUSK,
        Edit::Set(
            "actions.grasping_wraps.cooldown_ms",
            "[16000, 14000, 12000, 10000]",
        ),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "grasping_wraps"),
                &ActionProblem::RankCount(5),
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            "[combat]\n",
            "orders = { ai = \"scripts/lash_out.rhai\", think_ms = 250 }\n[combat]\n",
        ),
        "hero-husk",
        |problem| rule(problem, &AiError::AvatarOrders),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace("[combat]\n", "[fight]\n"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `fight`"),
    ),
    // An action delivers a projectile type of its own package, which homes only alone and at a
    // unit, and needs an aim; a weapon's homes. A projectile type is a delivery type alone, a
    // dependency's unit types are all delivery types, and only actions make projectiles.
    Flaw::of(
        HUSK,
        Edit::Set("units.grasping_wraps.projectile.homing", "true"),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "grasping_wraps"),
                &ProjectileDeliveryError::Homing,
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            r#"delivery = { projectile = "grasping_wraps" }"#,
            r#"delivery = { projectile = "wraps" }"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "grasping_wraps" && name == "wraps"),
    ),
    Flaw::of(
        HUSK,
        Edit::Set("actions.grasping_wraps.targeting", r#""none""#),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "grasping_wraps"),
                &ProjectileDeliveryError::NoAim,
            )
        },
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            "[combat]\n",
            "projectile = { speed = \"20\" }\n\n[combat]\n",
        ),
        "hero-husk",
        |problem| {
            rule_of(problem, |at, error: &UnitTypeError| {
                matches!(at, Place::Avatar(_)) && *error == UnitTypeError::NotDelivery
            })
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Replace(
            "[units.tower_bolt]\n",
            "[units.tower_bolt]\npools = [\"health\"]\n",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &unit_type("tower_bolt"),
                &UnitTypeError::NotDelivery,
            )
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Remove("units.tower_bolt.projectile.homing"),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &CombatDataError::Delivery,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            r#"delivery = { projectile = "tower_bolt" }"#,
            r#"delivery = { projectile = "tower" }"#,
        ),
        MODE,
        |problem| {
            let wrong = ProjectileDeliveryError::NotProjectile(DeclaredName::new("tower").unwrap());
            rule_at(problem, &entry("actions", "tower_attack"), &wrong)
        },
    ),
    // An area delivery names an area type, which lands on a point, a unit or the caster, takes
    // no count, and holds modifiers of its package; a delivery type has one section of the two.
    Flaw::of(
        CINDER,
        Edit::Replace(
            r#"delivery = { area = "eruption" }"#,
            r#"delivery = { area = "fire_lance" }"#,
        ),
        "hero-cinder",
        |problem| {
            let wrong = AreaDeliveryError::NotArea(DeclaredName::new("fire_lance").unwrap());
            rule_at(problem, &entry("actions", "eruption"), &wrong)
        },
    ),
    Flaw::of(
        CINDER,
        Edit::Set("actions.eruption.targeting", r#""direction""#),
        "hero-cinder",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "eruption"),
                &AreaDeliveryError::Direction,
            )
        },
    ),
    Flaw::of(
        CINDER,
        Edit::Replace(
            r#"delivery = { area = "eruption" }"#,
            r#"delivery = { area = "eruption", count = 2 }"#,
        ),
        "hero-cinder",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `count`"),
    ),
    // A delivery names one form of those the modules register, not two or none.
    Flaw::of(
        CINDER,
        Edit::Replace(
            r#"delivery = { area = "eruption" }"#,
            r#"delivery = { area = "eruption", projectile = "eruption" }"#,
        ),
        "hero-cinder",
        |problem| read_fails(problem, "data/avatar.toml", "a delivery names one of"),
    ),
    Flaw::of(
        CINDER,
        Edit::Replace(
            r#"delivery = { area = "eruption" }"#,
            "delivery = { count = 2 }",
        ),
        "hero-cinder",
        |problem| read_fails(problem, "data/avatar.toml", "a delivery names one of"),
    ),
    Flaw::of(
        CINDER,
        Edit::Set("units.eruption.area.affects", r#""enemies:molten""#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, at: Place::UnitType(name), .. } if name == "eruption"),
    ),
    Flaw::of(
        CINDER,
        Edit::Replace(
            "[units.eruption]\n",
            "[units.eruption]\nprojectile = { speed = \"20\" }\n",
        ),
        "hero-cinder",
        |problem| rule_at(problem, &unit_type("eruption"), &UnitTypeError::NotDelivery),
    ),
    Flaw::of(
        VEIL,
        Edit::Replace(r#"self = "smoke_ring_cover""#, r#"self = "smoke_cover""#),
        "hero-veil",
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::UnitType(unit_type), name } if unit_type == "smoke_ring" && name == "smoke_cover"),
    ),
    Flaw::of(
        MANIFEST,
        Edit::Replace(r#""areas", "#, ""),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Areas, at: Place::UnitType(name) } if name == "eruption"),
    ),
    Flaw::of(
        MAP,
        Edit::Replace("unit_type = \"tower\"", "unit_type = \"tower_bolt\""),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownUnitType(DeclaredName::new("tower_bolt").unwrap()),
            )
        },
    ),
    // Snow Owl hits nothing: a width, a stop on hit, a hit once a cast or homing would use hits,
    // and so would an `on_hit` list or hook of its action.
    Flaw::of(
        RIME,
        Edit::Set("units.snow_owl.projectile.width", r#""0.5""#),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set("units.snow_owl.projectile.stop_on_hit", "true"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set("units.snow_owl.projectile.once_per_cast", "true"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set("units.snow_owl.projectile.homing", "true"),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &unit_type("snow_owl"),
                &ProjectileError::HitsNothing,
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set(
            "actions.snow_owl.on_hit",
            r#"[{ damage = { amount = 1, kind = "true" } }]"#,
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "snow_owl"),
                &ProjectileDeliveryError::NoHit,
            )
        },
    ),
    Flaw::of(
        SNOW_OWL,
        Edit::Replace(
            "fn on_end(ctx, caster, hit) {",
            "fn on_hit(ctx, caster, target, hit) {}\n\nfn on_end(ctx, caster, hit) {",
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "snow_owl"),
                &ProjectileDeliveryError::NoHit,
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set("units.snow_owl.projectile.hits", r#""nobody""#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", r#"filter "nobody""#),
    ),
    // A launch names an area type of its own package, and its `on_end` reaches no unit.
    Flaw::of(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ land = { area = "fire_lance" } }]"#,
        ),
        "hero-cinder",
        |problem| {
            let wrong = AreaDeliveryError::NotArea(DeclaredName::new("fire_lance").unwrap());
            rule_at(problem, &entry("actions", "attack"), &wrong)
        },
    ),
    Flaw::of(
        CINDER,
        Edit::Set("actions.attack.on_hit", r#"[{ land = { area = "pyre" } }]"#),
        "hero-cinder",
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "attack" && name == "pyre"),
    ),
    Flaw::of(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ land = { area = "eruption", on_end = [{ heal = { amount = 1 } }] } }]"#,
        ),
        "hero-cinder",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "attack"),
                &ActionProblem::Effect {
                    list: ListHook::OnEnd,
                    rule: EffectRule::NoUnit,
                },
            )
        },
    ),
    // An action spawns a unit type of its own package that stands, an avatar's summon among
    // them, for a whole number of milliseconds: Rime's arrow, a projectile, is none. A summon of
    // Rime's names only what the mode declares, and one of the spells' loadout slots its actions
    // only in kinds of the loadout's ranks. Loot is planned.
    Flaw::of(
        RIME,
        Edit::Replace(SLOWS, r#"{ spawn = { unit_type = "frost_arrow" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "frost_arrow"),
    ),
    Flaw::of(
        RIME,
        Edit::Set("units.wisp.stats.nonsense.base", "1"),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: StatId::NAMES, name, .. } if name == "nonsense"),
    ),
    Flaw::of(
        "spells/data/loadout.toml",
        Edit::Set("units.totem.slots.basic", r#"["blink"]"#),
        "player-spells",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "blink"),
                &ActionProblem::SlottedRanks,
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace(SLOWS, r#"{ loot = { table = "chest", level = 1 } },"#),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "fan_of_frost"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Planned(PlannedEffect::Loot),
                },
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.on_hit",
            r#"[{ spawn = { unit_type = "caster_creep_bolt" } }]"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "caster_creep_bolt"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Set(
            "actions.tower_attack.on_hit",
            r#"[{ spawn = { unit_type = "melee_creep", duration_ms = "1.5" } }]"#,
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("actions", "tower_attack"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Duration,
                },
            )
        },
    ),
    // A move is a dash's `to` and `speed`, or a knock back's `from`, `distance` and whole `ms`;
    // its other unit, as its own, is one its list reaches.
    Flaw::of(
        RIME,
        Edit::Replace(SLOWS, r#"{ move = { to = "source", distance = 1 } },"#),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", "a dash's"),
    ),
    Flaw::of(
        RIME,
        Edit::Replace(
            SLOWS,
            r#"{ move = { from = "source", distance = 1, ms = "0.5" } },"#,
        ),
        "hero-rime",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "fan_of_frost"),
                &ActionProblem::Effect {
                    list: ListHook::OnHit,
                    rule: EffectRule::Duration,
                },
            )
        },
    ),
    Flaw::of(
        CINDER,
        Edit::Set(
            "actions.attack.on_hit",
            r#"[{ land = { area = "eruption", on_end = [{ move = { to = "reached", speed = 5 }, to = "source" }] } }]"#,
        ),
        "hero-cinder",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "attack"),
                &ActionProblem::Effect {
                    list: ListHook::OnEnd,
                    rule: EffectRule::NoUnit,
                },
            )
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "frozen" } },"#),
        "hero-rime",
        |problem| matches!(problem, LoadProblem::Unknown { of: Tag::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "fan_of_frost" && name == "frozen"),
    ),
    Flaw::of(
        RIME,
        Edit::Replace(SLOWS, r#"{ purge = { tag = "avatar" } },"#),
        "hero-rime",
        |problem| {
            let purged = EffectRule::EngineTag(DeclaredName::new("avatar").unwrap());
            let effect = ActionProblem::Effect {
                list: ListHook::OnHit,
                rule: purged,
            };
            rule_at(problem, &entry("actions", "fan_of_frost"), &effect)
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace(
            SLOWS,
            r#"{ heal = { amount = 1 }, restore = { pool = "mana", amount = 1 } },"#,
        ),
        "hero-rime",
        |problem| read_fails(problem, "data/avatar.toml", "exactly one effect"),
    ),
    Flaw::of(
        RIME,
        Edit::Replace("on_hit = [", "on_end = ["),
        "hero-rime",
        |problem| {
            let broken = EffectRule::NoUnit;
            let effect = ActionProblem::Effect {
                list: ListHook::OnEnd,
                rule: broken,
            };
            rule(problem, &effect)
        },
    ),
    Flaw::of(
        RIME,
        Edit::Replace("on_hit = [", "on_resolve = ["),
        "hero-rime",
        |problem| {
            let broken = EffectRule::NoUnit;
            let effect = ActionProblem::Effect {
                list: ListHook::OnResolve,
                rule: broken,
            };
            rule(problem, &effect)
        },
    ),
    Flaw::of(
        RIME,
        Edit::Remove("actions.fan_of_frost.delivery"),
        "hero-rime",
        |problem| {
            let broken = EffectRule::NoDelivery;
            let effect = ActionProblem::Effect {
                list: ListHook::OnHit,
                rule: broken,
            };
            rule(problem, &effect)
        },
    ),
    Flaw::of(
        RIME,
        Edit::Set(
            "actions.fan_of_frost.params.damage.base",
            "[40, 50, -60, 70, 80]",
        ),
        "hero-rime",
        |problem| {
            rule_of(problem, |_, error: &ActionProblem| {
                matches!(
                    error,
                    ActionProblem::Effect {
                        rule: EffectRule::Negative,
                        ..
                    }
                )
            })
        },
    ),
];

#[test]
fn every_flaw_of_the_data_or_a_delivery_fails_its_load_with_its_own_problem() {
    fails_each(DATA_FLAWS, |edits| load(&edited(edits.iter().copied())));
}
