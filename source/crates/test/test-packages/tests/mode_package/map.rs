//! Each flaw a package can have fails the load of the MOBA packages with its own problem.

use crate::flaws::{Flaw, fails_each};
use campfire_actions::{ActionProblem, EffectRule, SlotKind};
use campfire_common::DeclaredName;
use campfire_declarations::NamesListError;
use campfire_mode::{ChoiceId, MarkerTag, ModeDataError, ModeError, TimerId};
use campfire_navigation::NavigationDataError;
use campfire_orders::{AcquireError, RetaliationError};
use campfire_package::{LoadProblem, Place};
use campfire_production::ProductionDataError;
use campfire_script::Filter;
use campfire_sim::Capability;
use campfire_test_packages::Edit;
use campfire_units::Layer;
use campfire_values::{ModifierId, Named, ResourceId, TimeTooLarge, UnitType};

use crate::moba::{edited, load};
use crate::module_rules::{entry, rule, rule_at, rule_of, unit_type};

/// A flaw, the package it is in, and the problem it fails the load with.
use crate::mode_package::{
    GOLD_MINE, HUSK, LIVING_YARD, MANIFEST, MAP, MODE, MODE_DATA, MODE_SCRIPT, PRODUCTION, RECRUIT,
    UNITS, YARD, read_fails,
};

/// Flaws of the map, its layers, the slot kinds, production, gathering, a use, a player
/// resource, a player modifier, a relation and a loadout.
pub(super) static MAP_FLAWS: &[Flaw] = &[
    // A marker tag a script names is some marker's; a point fits the map's metric; a region is a
    // box within the bounds, of a marker with no point; a placed unit walks only a path it
    // names; marker names differ.
    Flaw::of(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(r#"ctx.map.markers("camp")"#, r#"ctx.map.markers("camps")"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: MarkerTag::NAMES, name: tag, .. } if tag == "camps"),
    ),
    Flaw::of(
        MAP,
        Edit::Set("markers.0.position", "[0, 0, -60]"),
        MODE,
        |problem| rule(problem, &ModeError::PointShape),
    ),
    Flaw {
        file: MAP,
        edit: Edit::Remove("markers.2.position"),
        also: &[(
            MAP,
            Edit::Set("markers.2.region", "{ min = [-20, -14], max = [-16, -70] }"),
        )],
        package: MODE,
        refused: |problem| {
            rule(
                problem,
                &ModeError::Region(DeclaredName::new("camp1").unwrap()),
            )
        },
    },
    Flaw::of(
        MAP,
        Edit::Set("markers.2.region", "{ min = [-20, -14], max = [-16, -10] }"),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::Region(DeclaredName::new("camp1").unwrap()),
            )
        },
    ),
    Flaw::of(
        MAP,
        Edit::Set("units.2.path", r#""middle""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownPath(DeclaredName::new("middle").unwrap()),
            )
        },
    ),
    Flaw::of(
        MAP,
        Edit::Set("units.0.from", r#""start""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::NoPathToWalk(DeclaredName::new("core").unwrap()),
            )
        },
    ),
    // A placed unit names a variation of its type, which the tower has none of.
    Flaw::of(MAP, Edit::Set("units.2.variation", "0"), MODE, |problem| {
        let variation = ModeError::NoVariation {
            unit_type: DeclaredName::new("tower").unwrap(),
            variation: 0,
        };
        rule(problem, &variation)
    }),
    Flaw::of(
        MAP,
        Edit::Set("markers.3.name", r#""camp1""#),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::RepeatedMarker(DeclaredName::new("camp1").unwrap()),
            )
        },
    ),
    // The layers have names of their own, a unit type moves on one of them, and they are
    // navigation's.
    Flaw::of(
        MODE_DATA,
        Edit::Set("navigation.layers", r#"["ground", "ground"]"#),
        MODE,
        |problem| {
            let ground = NamesListError::Repeated(DeclaredName::new("ground").unwrap());
            rule_at(problem, &Place::Section("navigation"), &ground)
        },
    ),
    // A type's idle scan names a filter of its targets at least, of the match's tags, and needs a
    // weapon and no build or gather (04-capabilities/control.md, A unit that scans neither builds
    // nor gathers).
    Flaw::of(
        UNITS,
        Edit::Set("units.tower.acquire", "{ scan_ms = 250, targets = [] }"),
        MODE,
        |problem| rule_at(problem, &unit_type("tower"), &AcquireError::NoTargets),
    ),
    Flaw::of(
        UNITS,
        Edit::Set(
            "units.tower.acquire",
            r#"{ scan_ms = 250, targets = ["enemies:flying"] }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, at: Place::UnitType(unit), name } if unit == "tower" && name == "enemies:flying"),
    ),
    Flaw::of(
        UNITS,
        Edit::Set(
            "units.inhibitor.acquire",
            r#"{ scan_ms = 250, targets = ["enemies"] }"#,
        ),
        MODE,
        |problem| rule_at(problem, &unit_type("inhibitor"), &AcquireError::NoWeapon),
    ),
    // A type that retaliates has a weapon and walks (combat-response.md, Retaliation is the
    // mode's data).
    Flaw::of(
        UNITS,
        Edit::Set(
            "units.inhibitor.retaliate",
            r#"{ leash = "10", reach = "20" }"#,
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &unit_type("inhibitor"),
                &RetaliationError::NoWeapon,
            )
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Set("units.tower.retaliate", r#"{ leash = "10", reach = "20" }"#),
        MODE,
        |problem| rule_at(problem, &unit_type("tower"), &RetaliationError::NoWalk),
    ),
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            r#"slots = { weapon = ["tower_attack"] }"#,
            r#"slots = { weapon = ["tower_attack", "mine"] }
acquire = { scan_ms = 250, targets = ["enemies"] }"#,
        ),
        also: &[
            (MANIFEST, PRODUCTION),
            (UNITS, GOLD_MINE),
            (
                MODE_DATA,
                Edit::Replace(
                    "[actions.melee_creep_attack]",
                    "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\n\n[actions.melee_creep_attack]",
                ),
            ),
        ],
        package: MODE,
        refused: |problem| rule_at(problem, &unit_type("tower"), &AcquireError::Works),
    },
    Flaw::of(
        UNITS,
        Edit::Set("units.melee_creep.collision.layer", r#""air""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Layer::NAMES, at: Place::UnitType(unit_type), name: layer } if unit_type == "melee_creep" && layer.as_str() == "air"),
    ),
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace(r#""orders", "navigation", "vision""#, r#""vision""#),
        also: &[(MODE_DATA, Edit::Set("navigation.layers", r#"["ground"]"#))],
        package: MODE,
        refused: |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Navigation,
                    at: Place::Section("navigation")
                }
            )
        },
    },
    // Slot kinds have names of their own; a unit type fills only kinds the mode declares, with
    // each of its abilities once; a choice of loadout entries fills a kind, and only one; all
    // such kinds have one count of ranks; scripts name only the mode's choices and kinds.
    Flaw::of(
        HUSK,
        Edit::Replace(
            r#"ultimate = ["tomb_bind"]"#,
            r#"ultimates = ["tomb_bind"]"#,
        ),
        "hero-husk",
        |problem| matches!(problem, LoadProblem::Unknown { of: SlotKind::NAMES, at: Place::Avatar(avatar), name } if avatar == "hero-husk" && name == "ultimates"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            r#""grasping_wraps", "dread", "lash_out""#,
            r#""grasping_wraps", "lash_out""#,
        ),
        "hero-husk",
        |problem| {
            rule_at(
                problem,
                &entry("actions", "dread"),
                &ActionProblem::Unslotted,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("name = \"ultimate\"", "name = \"basic\""),
        MODE,
        |problem| read_fails(problem, "data/mode.toml", "the slot kind `basic` twice"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Remove("choices.spells.slot"),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "spells"),
                &ModeDataError::ChoiceSlot,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "unique = true\n\n[choices.spells]",
            "unique = true\nslot = \"spell\"\n\n[choices.spells]",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "hero"),
                &ModeDataError::ChoiceSlot,
            )
        },
    ),
    // A choice of values lists them, each once, and takes one; a choice of avatars lists none.
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "unique = true\n\n[choices.spells]",
            "unique = true\nvalues = [\"north\"]\n\n[choices.spells]",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "hero"),
                &ModeDataError::ChoiceValues,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[choices.spells]",
            "[choices.side]\nname = \"choice-hero\"\noffers = \"values\"\n\n[choices.spells]",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "side"),
                &ModeDataError::ChoiceValues,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[choices.spells]",
            "[choices.side]\nname = \"choice-hero\"\noffers = \"values\"\nvalues = [\"north\", \"north\"]\n\n[choices.spells]",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "side"),
                &ModeDataError::ChoiceValues,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[choices.spells]",
            "[choices.side]\nname = \"choice-hero\"\noffers = \"values\"\nvalues = [\"north\", \"south\"]\ncount = 2\n\n[choices.spells]",
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &entry("choices", "side"),
                &ModeDataError::ChoiceValues,
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace("slot = \"spell\"", "slot = \"spells\""),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: SlotKind::NAMES, at: Place::Entry { table: "choices", name: choice }, name } if choice.as_str() == "spells" && name == "spells"),
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "slot = \"spell\"",
            "slot = \"spell\"\n\n[choices.more]\nname = \"choice-spells\"\noffers = \"loadout\"\nslot = \"basic\"",
        ),
        MODE,
        |problem| rule(problem, &ModeDataError::LoadoutRanks),
    ),
    Flaw::of(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"ctx.chosen(player, "spells")"#,
            r#"ctx.chosen(player, "spell")"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ChoiceId::NAMES, name, .. } if name == "spell"),
    ),
    Flaw::of(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(r#"ctx.after("pick_end""#, r#"ctx.after("pick_ends""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TimerId::NAMES, name, .. } if name == "pick_ends"),
    ),
    // A literal compared with the name a hook passes, or found in a list of names, is a name of
    // its kind: `on_timer`'s timer, and a unit type in a queue.
    Flaw::of(
        MODE_SCRIPT,
        Edit::Replace(r#"if name == "pick_end" {"#, r#"if name == "pick_ends" {"#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: TimerId::NAMES, name, .. } if name == "pick_ends"),
    ),
    Flaw {
        file: MODE_SCRIPT,
        edit: Edit::Replace(
            r#"    if name == "pick_end" {"#,
            r#"    for unit in ctx.all_avatars() {
        if "melee_creeps" in unit.queue {}
    }
    if name == "pick_end" {"#,
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "melee_creeps"),
    },
    Flaw::of(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"ctx.grant(unit, "spell","#,
            r#"ctx.grant(unit, "spells","#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: SlotKind::NAMES, name, .. } if name == "spells"),
    ),
    // A train is production's, names a unit type of the mode and takes no target, and sits on a
    // unit type with a queue; no other kind names a unit type.
    Flaw::of(
        MODE_DATA,
        Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::Entry { table: "actions", name: action } } if action == "recruit"),
    ),
    // A train needs the name players see on its producer's button.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::UnnamedAction(action) if action == "recruit"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                MODE_DATA,
                Edit::Replace(r#"unit_type = "melee_creep""#, r#"unit_type = "knight""#),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "knight"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                MODE_DATA,
                Edit::Replace(
                    "targeting = \"none\"\nunit_type",
                    "targeting = \"enemies\"\nunit_type",
                ),
            ),
        ],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "recruit"),
                &ProductionDataError::TrainAims,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                UNITS,
                Edit::Replace(
                    r#"weapon = ["tower_attack"]"#,
                    r#"weapon = ["tower_attack", "recruit"]"#,
                ),
            ),
        ],
        package: MODE,
        refused: |problem| {
            let recruit = ProductionDataError::NoQueue(DeclaredName::new("recruit").unwrap());
            rule_at(problem, &unit_type("tower"), &recruit)
        },
    },
    // A box on a creep, which walks; on a ward, which an effect spawns; and a train of a tower,
    // which does not walk.
    Flaw::of(
        UNITS,
        Edit::Replace(
            r#"collision = { radius = "0.35" }"#,
            r#"collision = { box = ["1", "1"] }"#,
        ),
        MODE,
        |problem| {
            rule_at(
                problem,
                &unit_type("melee_creep"),
                &NavigationDataError::BoxWalks,
            )
        },
    ),
    Flaw::of(
        UNITS,
        Edit::Replace(
            r#"tags = ["ward", "stealthed"]"#,
            "tags = [\"ward\", \"stealthed\"]\ncollision = { box = [\"0.5\", \"0.5\"] }",
        ),
        MODE,
        |problem| {
            rule_of(problem, |_, error: &ActionProblem| {
                matches!(
                    error,
                    ActionProblem::Effect {
                        rule: EffectRule::SpawnBox,
                        ..
                    }
                )
            })
        },
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace("[actions.melee_creep_attack]", RECRUIT),
        also: &[
            (MANIFEST, PRODUCTION),
            (
                MODE_DATA,
                Edit::Replace(r#"unit_type = "melee_creep""#, r#"unit_type = "tower""#),
            ),
        ],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "recruit"),
                &ProductionDataError::TrainStands,
            )
        },
    },
    // A train's `requires` names unit types of its package that stand, and modifiers of its
    // package; a cast takes none. A unit type's `supply` needs the mode's `[supply]`.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"tower_bolt\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "recruit" && name == "tower_bolt"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"tower\"], modifiers = [\"drill\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "recruit" && name == "drill"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.recruit]\nkind = \"train\"\ntargeting = \"none\"\nunit_type = \"melee_creep\"\nrequires = { units = [\"keep\"] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "recruit" && name == "keep"),
    },
    // A unit type's passive is a modifier of its package.
    Flaw::of(
        UNITS,
        Edit::Set("units.tower.passive", r#""warden_blesing""#),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ModifierId::NAMES, at: Place::UnitType(unit_type), name } if unit_type == "tower" && name == "warden_blesing"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace("[actions.dread]\n", "[actions.dread]\nrequires = {}\n"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `requires`"),
    ),
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            r#"slots = { weapon = ["tower_attack"] }"#,
            "slots = { weapon = [\"tower_attack\"] }\nsupply = { provides = 10 }",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("tower"),
                &ProductionDataError::SupplyUncounted,
            )
        },
    },
    // A build aims at a point, within meters, of a unit type of its package with a body;
    // its site starts with a share of its life above 0 when the type has the life pool; its
    // construct is needed, and its placement's filters name the mode's tags.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"none\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::BuildAims,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"sight_ward\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::NoBody,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"dragon\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, name, .. } if name == "dragon"),
    },
    // A delivery type is no unit type of the mode that a build raises.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"caster_creep_bolt\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "raise" && name == "caster_creep_bolt"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\nstart_life = \"0\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::StartLife,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| read_fails(problem, "data/mode.toml", "missing field `construct`"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\nplacement = { near = [{ filter = \"allies:pylon\", distance = \"4\" }] }\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            matches!(
                problem,
                LoadProblem::Unknown {
                    of: Filter::NAMES,
                    ..
                }
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"2\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, LIVING_YARD)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "raise"),
                &ProductionDataError::StartLife,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.raise]\nkind = \"build\"\nrange = \"global\"\nwindup_ms = 1000\ntargeting = \"point\"\nunit_type = \"yard\"\nconstruct = \"alone\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, YARD)],
        package: MODE,
        refused: |problem| {
            let global = ActionProblem::Global;
            rule_at(problem, &entry("actions", "raise"), &global)
        },
    },
    // A gather aims at a unit its filter selects, within meters; it gathers a player resource
    // of the mode's, and its bounce is a distance; a node and a drop-off name the mode's
    // resources.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"point\"\nresource = \"gold\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "mine"),
                &ProductionDataError::GatherAims,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"global\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| {
            let global = ActionProblem::Global;
            rule_at(problem, &entry("actions", "mine"), &global)
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\nbounce = \"-1\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| read_fails(problem, "data/mode.toml", "a distance is a number from 0"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"silver\"\ntake = 5\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, at: Place::Entry { table: "actions", name: action }, name } if action == "mine" && name == "silver"),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| read_fails(problem, "data/mode.toml", "missing field `take`"),
    },
    // A gather's unit takes at most its trip's take, and its time at a drop-off counts in ticks.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\nunit = 6\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "mine"),
                &ProductionDataError::UnitPastTake,
            )
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[actions.melee_creep_attack]",
            "[actions.mine]\nkind = \"gather\"\nrange = \"1\"\nwindup_ms = 1000\ntargeting = \"neutrals\"\nresource = \"gold\"\ntake = 5\ndeliver_ms = 9000000000000000000\n\n[actions.melee_creep_attack]",
        ),
        also: &[(MANIFEST, PRODUCTION), (UNITS, GOLD_MINE)],
        package: MODE,
        refused: |problem| rule_at(problem, &entry("actions", "mine"), &TimeTooLarge),
    },
    // A producer frees unit types of its package that walk and that no action delivers.
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\nproduction = { queue = 1, free = [\"golem\"] }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: UnitType::NAMES, at: Place::UnitType(unit), name } if unit == "yard" && name == "golem"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\nproduction = { queue = 1, free = [\"tower\"] }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::LeaverStands,
            )
        },
    },
    // A kept spawn's type lives, so its units die to come back; a type that keeps spawns needs
    // no production section, and takes one unit type or more.
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\nspawns = { unit_types = [\"imp\"], count = 2, replace_ms = 1000 }\n\n[units.imp]\nstats = { move_speed = { base = \"3\" } }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::SpawnUnliving,
            )
        },
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\nspawns = { unit_types = [], count = 2, replace_ms = 1000 }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::SpawnTypes(0),
            )
        },
    },
    // A leader of its spawns has no life, body or walk of its own (04-capabilities/production.md, A
    // unit may lead its spawns).
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\npools = [\"health\"]\nstats = { health = { base = 10 } }\ncombat = {}\nspawns = { unit_types = [\"melee_creep\"], count = 2, replace_ms = 1000, led = true }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::LeaderLives,
            )
        },
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\ncollision = { radius = \"1\" }\nspawns = { unit_types = [\"melee_creep\"], count = 2, replace_ms = 1000, led = true }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::LeaderBody,
            )
        },
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\nstats = { move_speed = { base = \"3\" } }\nspawns = { unit_types = [\"melee_creep\"], count = 2, replace_ms = 1000, led = true }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::LeaderWalks,
            )
        },
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.yard]\nproduction = { queue = 1, free = [\"caster_creep_bolt\"] }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &unit_type("yard"),
                &ProductionDataError::LeaverDelivered,
            )
        },
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.mine]\nnode = { resource = \"silver\", amount = 1500 }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, at: Place::UnitType(unit), name } if unit == "mine" && name == "silver"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Replace(
            "[units.tower]\n",
            "[units.depot]\ndrop_off = { resources = [\"gold\", \"silver\"] }\n\n[units.tower]\n",
        ),
        also: &[(MANIFEST, PRODUCTION)],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, at: Place::UnitType(unit), name } if unit == "depot" && name == "silver"),
    },
    Flaw::of(
        UNITS,
        Edit::Replace(
            "[units.tower]\n",
            "[units.mine]\nnode = { resource = \"gold\", amount = 1500 }\n\n[units.tower]\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::UnitType(name) } if name == "mine"),
    ),
    Flaw::of(
        UNITS,
        Edit::Replace(
            r#"slots = { weapon = ["tower_attack"] }"#,
            "slots = { weapon = [\"tower_attack\"] }\nproduction = { queue = 5 }",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Undeclared { capability: Capability::Production, at: Place::UnitType(name) } if name == "tower"),
    ),
    Flaw::of(
        HUSK,
        Edit::Replace(
            "[actions.dread]\n",
            "[actions.dread]\nunit_type = \"melee_creep\"\n",
        ),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown field `unit_type`"),
    ),
    // An action's kind is one the engine has; an action sits in slot kinds of one count of
    // ranks.
    Flaw::of(
        HUSK,
        Edit::Replace("[actions.dread]\n", "[actions.dread]\nkind = \"ride\"\n"),
        "hero-husk",
        |problem| read_fails(problem, "data/avatar.toml", "unknown variant `ride`"),
    ),
    Flaw {
        file: MODE_DATA,
        edit: Edit::Replace(
            "[modifiers.warden_blessing]",
            "[actions.taunt]\ntargeting = \"none\"\n\n[modifiers.warden_blessing]",
        ),
        also: &[
            (
                UNITS,
                Edit::Replace(
                    "slots = { weapon = [\"melee_creep_attack\"] }",
                    "slots = { weapon = [\"melee_creep_attack\"], basic = [\"taunt\"] }",
                ),
            ),
            (
                UNITS,
                Edit::Replace(
                    "slots = { weapon = [\"caster_creep_attack\"] }",
                    "slots = { weapon = [\"caster_creep_attack\"], ultimate = [\"taunt\"] }",
                ),
            ),
        ],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "taunt"),
                &ActionProblem::SlottedRanks,
            )
        },
    },
    // A script adds only to a player resource the mode declares.
    Flaw::of(
        "modes/3v3/scripts/mode.rhai",
        Edit::Replace(
            r#"add_resource(killer.owner, "gold", gold)"#,
            r#"add_resource(killer.owner, "silver", gold)"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, name, .. } if name == "silver"),
    ),
    // A type's bounty pays a player resource the mode declares.
    Flaw::of(
        UNITS,
        Edit::Replace(
            r#"bounty = { resource = "gold", amount = 20 }"#,
            r#"bounty = { resource = "silver", amount = 20 }"#,
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: ResourceId::NAMES, name, .. } if name == "silver"),
    ),
    // A player modifier's filter names a tag the mode has.
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "[modifiers.warden_blessing]\n",
            "[modifiers.warden_blessing]\naffects = \"allies:totem\"\n",
        ),
        MODE,
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "allies:totem"),
    ),
    // A relation names two of the mode's teams, a pair once.
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]\n",
            "resources = [\"gold\"]\n\n[[relations]]\nteams = [\"north\", \"east\"]\nrelation = \"neutral\"\n",
        ),
        MODE,
        |problem| {
            rule(
                problem,
                &ModeError::UnknownTeam(DeclaredName::new("east").unwrap()),
            )
        },
    ),
    Flaw::of(
        MODE_DATA,
        Edit::Replace(
            "resources = [\"gold\"]\n",
            "resources = [\"gold\"]\n\n[[relations]]\nteams = [\"north\", \"camps\"]\nrelation = \"neutral\"\n\n[[relations]]\nteams = [\"camps\", \"north\"]\nrelation = \"friendly\"\n",
        ),
        MODE,
        |problem| {
            let name = |name| DeclaredName::new(name).unwrap();
            rule(
                problem,
                &ModeError::RepeatedRelation(name("camps"), name("north")),
            )
        },
    ),
    Flaw::of(
        "heroes/kensho/data/avatar.toml",
        Edit::Replace(r#"targeting = "enemies""#, r#"targeting = "enemies:totem""#),
        "hero-kensho",
        |problem| matches!(problem, LoadProblem::Unknown { of: Filter::NAMES, name: filter, .. } if filter == "enemies:totem"),
    ),
    // A second spells package with a spell the first holds.
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace(
            "[dependencies]\n",
            "[dependencies]\nmore-spells = { path = \"../../more\" }\n",
        ),
        also: &[
            (
                "more/manifest.toml",
                Edit::Create(
                    "name = \"more-spells\"\nversion = \"0.1.0\"\napi = \"1.0\"\nlanguage = \"en\"\nkind = \"loadout\"\n",
                ),
            ),
            (
                "more/data/loadout.toml",
                Edit::Create("[actions.haste]\ntargeting = \"none\"\n"),
            ),
        ],
        package: "player-spells",
        refused: |problem| matches!(problem, LoadProblem::Repeated { at: Place::Loadouts, name } if name == "haste"),
    },
];

#[test]
fn every_flaw_of_the_map_or_the_economy_fails_its_load_with_its_own_problem() {
    fails_each(MAP_FLAWS, |edits| load(&edited(edits.iter().copied())));
}
