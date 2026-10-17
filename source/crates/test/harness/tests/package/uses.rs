//! The uses in the 3v3: a fountain that the hero Husk uses with a prayer, which loads, and each
//! flaw of a use, an object or a user, which fails the load with its own problem.

use campfire_actions::{ActionProblem, PipelineField, SlotError};
use campfire_common::DeclaredName;
use campfire_interaction::UseError;
use campfire_items::ItemsDataError;
use campfire_package::{LoadError, LoadProblem, ModePackages, PackageName, PackageRef, Place};
use campfire_script::NameKind;
use campfire_sim::Capability;
use campfire_test_packages::Edit;

use crate::package::moba::{edited, load};
use crate::package::module_rules::{entry, rule_at};

const MANIFEST: &str = "modes/3v3/manifest.toml";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";
const UNITS: &str = "modes/3v3/data/units.toml";
const HUSK: &str = "heroes/husk/data/avatar.toml";
const SPELLS: &str = "spells/data/loadout.toml";
const MODE: &str = "moba-3v3";

/// The 3v3 with `interaction`, a slot kind `use` after the items' kind, a prayer of the mode, a
/// use of 2 m that winds up 1 s, the fountain's use, and Husk's use slot.
const USES: [(&str, Edit<'static>); 5] = [
    (
        MANIFEST,
        Edit::Replace(r#", "items"]"#, r#", "items", "interaction"]"#),
    ),
    (
        MODE_DATA,
        Edit::Replace(
            "[[slots]]\nname = \"item\"\n",
            "[[slots]]\nname = \"item\"\n\n[[slots]]\nname = \"use\"\n",
        ),
    ),
    (
        MODE_DATA,
        Edit::Set(
            "actions.pray",
            r#"{ kind = "use", targeting = "all", range = "2.0", windup_ms = 1000 }"#,
        ),
    ),
    (
        UNITS,
        Edit::Set("units.fountain.use", r#"{ action = "pray" }"#),
    ),
    (HUSK, Edit::Set("uses", r#"{ kind = "use" }"#)),
];

/// The 3v3 with its uses and `more` edits after them.
fn with_uses(more: &[(&'static str, Edit<'static>)]) -> Result<ModePackages, LoadError> {
    load(&edited(USES.iter().chain(more).copied()))
}

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

fn fountain() -> Place {
    Place::UnitType(name("fountain"))
}

fn husk() -> Place {
    Place::Avatar(PackageName::new("hero-husk").unwrap())
}

/// A flaw of the uses: its edits, after the uses', the package the load names, and whether a
/// problem is the one the flaw makes.
#[derive(Debug)]
struct Flaw {
    edits: &'static [(&'static str, Edit<'static>)],
    package: &'static str,
    refused: fn(&LoadProblem) -> bool,
}

/// Whether `problem` is the prayer's data giving `field`, which its kind refuses or needs.
fn misused(problem: &LoadProblem, field: PipelineField) -> bool {
    rule_at(
        problem,
        &entry("actions", "pray"),
        &ActionProblem::KindField(field),
    )
}

const FLAWS: [Flaw; 21] = [
    // A use has one rank and no cooldown, charges, toggle or charge, as its slot fills afresh; it
    // needs a range, in meters, and aims at a unit.
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("actions.pray.cooldown_ms", "1000"))],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::CooldownMs),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("actions.pray.charges", "{ max = 2, recharge_ms = 1000 }"),
        )],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Charges),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("actions.pray.toggle", "{ cost_per_second = { mana = 1 } }"),
        )],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Toggle),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("actions.pray.charge", "{ max_ms = 1000 }"),
        )],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Charge),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Remove("actions.pray.range"))],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Range),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("actions.pray.range", r#""global""#))],
        package: MODE,
        refused: |problem| {
            let global = ActionProblem::Global;
            rule_at(problem, &entry("actions", "pray"), &global)
        },
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("actions.pray.targeting", r#""point""#))],
        package: MODE,
        refused: |problem| rule_at(problem, &entry("actions", "pray"), &UseError::Aims),
    },
    // A use section names a use of its package, and users among the match's tags.
    Flaw {
        edits: &[(UNITS, Edit::Set("units.fountain.use.action", r#""harden""#))],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &UseError::NotUse(name("harden"))),
    },
    Flaw {
        edits: &[(UNITS, Edit::Set("units.fountain.use.action", r#""kneel""#))],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { at, of: NameKind::Ability, name } if *at == fountain() && name == "kneel"),
    },
    Flaw {
        edits: &[(
            UNITS,
            Edit::Set("units.fountain.use.users", r#""allies:pilgrim""#),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { at, of: NameKind::Filter, name } if *at == fountain() && name == "allies:pilgrim"),
    },
    // Only a use section names a use: no unit type's slots, no item, and no loadout entry.
    Flaw {
        edits: &[(
            UNITS,
            Edit::Set("units.wolf.slots.weapon", r#"["wolf_attack", "pray"]"#),
        )],
        package: MODE,
        refused: |problem| {
            rule_at(
                problem,
                &Place::UnitType(name("wolf")),
                &UseError::Named(name("pray")),
            )
        },
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.health_potion.action", r#""pray""#),
        )],
        package: MODE,
        refused: |problem| {
            let at = entry("items", "health_potion");
            rule_at(problem, &at, &ItemsDataError::UseAction(name("pray")))
        },
    },
    Flaw {
        edits: &[(
            SPELLS,
            Edit::Set(
                "actions.vow",
                r#"{ name = "action-blink", kind = "use", targeting = "all", range = "2.0" }"#,
            ),
        )],
        package: "player-spells",
        refused: |problem| {
            rule_at(
                problem,
                &entry("actions", "vow"),
                &UseError::Named(name("vow")),
            )
        },
    },
    // A use slot is of a kind the mode declares, of one rank, that no inventory fills, and in
    // which no unit type, an avatar's summon among them, and no choice puts actions.
    Flaw {
        edits: &[(HUSK, Edit::Set("uses.kind", r#""relic""#))],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Unknown { at, of: NameKind::SlotKind, name } if *at == husk() && name == "relic"),
    },
    Flaw {
        edits: &[(HUSK, Edit::Set("uses.kind", r#""basic""#))],
        package: "hero-husk",
        refused: |problem| rule_at(problem, &husk(), &SlotError::RankedClaim(name("basic"))),
    },
    Flaw {
        edits: &[(HUSK, Edit::Set("uses.kind", r#""item""#))],
        package: "hero-husk",
        refused: |problem| rule_at(problem, &husk(), &SlotError::ClaimedTwice(name("item"))),
    },
    Flaw {
        edits: &[(
            HUSK,
            Edit::Set(
                "slots",
                r#"{ basic = ["grasping_wraps", "lash_out"], ultimate = ["tomb_bind"], weapon = ["attack"], use = ["dread"] }"#,
            ),
        )],
        package: "hero-husk",
        refused: |problem| {
            let slotted = SlotError::ClaimedSlotted {
                kind: name("use"),
                by: Capability::Interaction,
            };
            rule_at(problem, &husk(), &slotted)
        },
    },
    Flaw {
        edits: &[
            (
                HUSK,
                Edit::Set(
                    "actions.mourn",
                    r#"{ name = "action-dread", targeting = "none" }"#,
                ),
            ),
            (
                HUSK,
                Edit::Set("units.mourner", r#"{ slots = { use = ["mourn"] } }"#),
            ),
        ],
        package: "hero-husk",
        refused: |problem| {
            let slotted = SlotError::ClaimedSlotted {
                kind: name("use"),
                by: Capability::Interaction,
            };
            rule_at(problem, &Place::UnitType(name("mourner")), &slotted)
        },
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("choices.spells.slot", r#""use""#))],
        package: MODE,
        refused: |problem| {
            let slotted = SlotError::ClaimedSlotted {
                kind: name("use"),
                by: Capability::Interaction,
            };
            rule_at(problem, &entry("choices", "spells"), &slotted)
        },
    },
    // A unit holds 256 slots at most: Husk's own 5, its inventory's and its use slot.
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.slots", "251"))],
        package: "hero-husk",
        refused: |problem| rule_at(problem, &husk(), &SlotError::TooMany),
    },
    // Uses need `interaction`.
    Flaw {
        edits: &[(
            MANIFEST,
            Edit::Replace(r#", "items", "interaction"]"#, r#", "items"]"#),
        )],
        package: MODE,
        refused: |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Interaction,
                    ..
                }
            )
        },
    },
];

#[test]
fn every_flaw_of_the_uses_fails_the_load_with_its_own_problem() {
    // The uses load, and Husk's 5 slots, 250 of its inventory and its use slot are 256.
    with_uses(&[]).unwrap();
    with_uses(&[(HUSK, Edit::Set("inventory.slots", "250"))]).unwrap();
    for flaw in &FLAWS {
        let Err(error) = with_uses(flaw.edits) else {
            panic!("{flaw:?} loads");
        };
        let LoadError { package, problem } = &error;
        let named = matches!(package, PackageRef::Name(name) if name == flaw.package);
        assert!(named && (flaw.refused)(problem), "{flaw:?}: {error:?}");
    }
}
