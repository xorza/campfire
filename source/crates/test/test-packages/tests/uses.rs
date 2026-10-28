//! The uses in the 3v3: a fountain that the hero Husk uses with a prayer, which loads, and each
//! flaw of a use, an object or a user, which fails the load with its own problem.

use crate::flaws::{Flaw, fails_each};
use campfire_actions::{
    ActionBook, ActionProblem, EffectListBook, ListHook, ListsOf, PipelineField, SlotError,
    SlotKind,
};
use campfire_common::DeclaredName;
use campfire_interaction::{EnterError, UseError};
use campfire_items::ItemsDataError;
use campfire_package::{LoadError, LoadProblem, ModePackages, Place};
use campfire_script::Filter;
use campfire_sim::Capability;
use campfire_test_packages::Edit;
use campfire_values::PackageName;
use campfire_values::{ActionId, Named, PackageIndex};

use crate::moba::{edited, load};
use crate::module_rules::{entry, rule_at};

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
        file: MODE_DATA,
        edit: Edit::Set("actions.pray.cooldown_ms", "1000"),
        also: &[],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::CooldownMs),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Set("actions.pray.charges", "{ max = 2, recharge_ms = 1000 }"),
        also: &[],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Charges),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Set("actions.pray.toggle", "{ cost_per_second = { mana = 1 } }"),
        also: &[],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Toggle),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Set("actions.pray.charging", "{ max_ms = 1000 }"),
        also: &[],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Charging),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Remove("actions.pray.range"),
        also: &[],
        package: MODE,
        refused: |problem| misused(problem, PipelineField::Range),
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Set("actions.pray.range", r#""global""#),
        also: &[],
        package: MODE,
        refused: |problem| {
            let global = ActionProblem::Global;
            rule_at(problem, &entry("actions", "pray"), &global)
        },
    },
    Flaw {
        file: MODE_DATA,
        edit: Edit::Set("actions.pray.targeting", r#""point""#),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &entry("actions", "pray"), &UseError::Aims),
    },
    // A use section names a use of its package, and users among the match's tags.
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.fountain.use.action", r#""harden""#),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &UseError::NotUse(name("harden"))),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.fountain.use.action", r#""kneel""#),
        also: &[],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { at, of: ActionId::NAMES, name } if *at == fountain() && name == "kneel"),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.fountain.use.users", r#""allies:pilgrim""#),
        also: &[],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Unknown { at, of: Filter::NAMES, name } if *at == fountain() && name == "allies:pilgrim"),
    },
    // Only a use section names a use: no unit type's slots, no item, and no loadout entry.
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.wolf.slots.weapon", r#"["wolf_attack", "pray"]"#),
        also: &[],
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
        file: MODE_DATA,
        edit: Edit::Set("items.health_potion.action", r#""pray""#),
        also: &[],
        package: MODE,
        refused: |problem| {
            let at = entry("items", "health_potion");
            rule_at(problem, &at, &ItemsDataError::UseAction(name("pray")))
        },
    },
    Flaw {
        file: SPELLS,
        edit: Edit::Set(
            "actions.vow",
            r#"{ name = "action-blink", kind = "use", targeting = "all", range = "2.0" }"#,
        ),
        also: &[],
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
        file: HUSK,
        edit: Edit::Set("uses.kind", r#""relic""#),
        also: &[],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Unknown { at, of: SlotKind::NAMES, name } if *at == husk() && name == "relic"),
    },
    Flaw {
        file: HUSK,
        edit: Edit::Set("uses.kind", r#""basic""#),
        also: &[],
        package: "hero-husk",
        refused: |problem| rule_at(problem, &husk(), &SlotError::RankedClaim(name("basic"))),
    },
    Flaw {
        file: HUSK,
        edit: Edit::Set("uses.kind", r#""item""#),
        also: &[],
        package: "hero-husk",
        refused: |problem| rule_at(problem, &husk(), &SlotError::ClaimedTwice(name("item"))),
    },
    Flaw {
        file: HUSK,
        edit: Edit::Set(
            "slots",
            r#"{ basic = ["grasping_wraps", "lash_out"], ultimate = ["tomb_bind"], weapon = ["attack"], use = ["dread"] }"#,
        ),
        also: &[],
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
        file: HUSK,
        edit: Edit::Set(
            "actions.mourn",
            r#"{ name = "action-dread", targeting = "none" }"#,
        ),
        also: &[(
            HUSK,
            Edit::Set("units.mourner", r#"{ slots = { use = ["mourn"] } }"#),
        )],
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
        file: MODE_DATA,
        edit: Edit::Set("choices.spells.slot", r#""use""#),
        also: &[],
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
        file: HUSK,
        edit: Edit::Set("inventory.slots", "251"),
        also: &[],
        package: "hero-husk",
        refused: |problem| rule_at(problem, &husk(), &SlotError::TooMany),
    },
    // Uses need `interaction`.
    Flaw {
        file: MANIFEST,
        edit: Edit::Replace(r#", "items", "interaction"]"#, r#", "items"]"#),
        also: &[],
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
    fails_each(&FLAWS, with_uses);
}

/// The 3v3's uses, a boarding of the mode, an entry of 1 m, the fountain a container of 2 that
/// boarding enters, and Husk a rider of size 1.
const ENTRIES: [(&str, Edit<'static>); 3] = [
    (
        MODE_DATA,
        Edit::Set(
            "actions.board",
            r#"{ kind = "enter", targeting = "all", range = "1.0" }"#,
        ),
    ),
    (
        UNITS,
        Edit::Set(
            "units.fountain.enter",
            r#"{ action = "board", capacity = 2 }"#,
        ),
    ),
    (HUSK, Edit::Set("rides", "{ size = 1 }")),
];

/// The 3v3 with its uses, its entries, and `more` edits after them.
fn with_entries(more: &[(&'static str, Edit<'static>)]) -> Result<ModePackages, LoadError> {
    let entries = USES.iter().chain(&ENTRIES).chain(more).copied();
    load(&edited(entries))
}

const ENTRY_FLAWS: [Flaw; 8] = [
    // A fire point lies within a place's bound, 2²⁰ m.
    Flaw {
        file: UNITS,
        edit: Edit::Set(
            "units.fountain.enter.fire",
            r#"{ points = [["1048577", "0"]] }"#,
        ),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &EnterError::FirePoint),
    },
    // An entry aims at a unit, and only an `enter` section names one, of the `enter` kind.
    Flaw {
        file: MODE_DATA,
        edit: Edit::Set("actions.board.targeting", r#""point""#),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &entry("actions", "board"), &EnterError::Aims),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.fountain.enter.action", r#""pray""#),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &EnterError::NotEnter(name("pray"))),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.wolf.slots.weapon", r#"["wolf_attack", "board"]"#),
        also: &[],
        package: MODE,
        refused: |problem| {
            let at = Place::UnitType(name("wolf"));
            rule_at(problem, &at, &EnterError::Named(name("board")))
        },
    },
    // A container holds a rider, and its payload rides and fits.
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.fountain.enter.capacity", "0"),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &EnterError::NoCapacity),
    },
    Flaw {
        file: UNITS,
        edit: Edit::Set(
            "units.fountain.enter.payload",
            r#"[{ unit = "wolf", count = 1 }]"#,
        ),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &EnterError::Payload),
    },
    // A rider has a use slot to enter by, and a size of at least 1.
    Flaw {
        file: UNITS,
        edit: Edit::Set("units.wolf.rides", "{ size = 1 }"),
        also: &[],
        package: MODE,
        refused: |problem| {
            let at = Place::UnitType(name("wolf"));
            rule_at(problem, &at, &EnterError::RidesWithoutUses)
        },
    },
    Flaw {
        file: HUSK,
        edit: Edit::Set("rides.size", "0"),
        also: &[],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Content(_)),
    },
];

#[test]
fn every_flaw_of_the_entries_fails_the_load_with_its_own_problem() {
    // The entries load, and the board's resolve holds the `enter` kind's own effect, though its
    // data writes no list.
    let packages = with_entries(&[]).unwrap();
    let books = packages.load_books();
    let names = books.get::<ActionBook>().unwrap().names();
    let board = names.named((PackageIndex::MODE, "board")).unwrap();
    let lists = books.get::<EffectListBook>().unwrap();
    let resolve = lists.of(ListsOf::Action(board, ListHook::OnResolve));
    assert_eq!(resolve.len(), 1);
    // A container holds 256 fire points, as a rider's point counts them, and no more.
    let points = |count: usize| -> &'static str {
        let points = vec![r#"["0", "0"]"#; count].join(", ");
        Box::leak(format!("{{ points = [{points}] }}").into_boxed_str())
    };
    with_entries(&[(UNITS, Edit::Set("units.fountain.enter.fire", points(256)))]).unwrap();
    let past = Flaw {
        file: UNITS,
        edit: Edit::Set("units.fountain.enter.fire", points(257)),
        also: &[],
        package: MODE,
        refused: |problem| rule_at(problem, &fountain(), &EnterError::FirePoint),
    };
    let flaws: Vec<Flaw> = ENTRY_FLAWS.into_iter().chain([past]).collect();
    fails_each(&flaws, with_entries);
}
