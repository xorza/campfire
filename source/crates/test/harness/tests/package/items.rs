//! The 3v3's item types, its fountains' shop and its heroes' inventories: each reads as its data
//! holds it, and each flaw of them fails the load with its own problem.

use std::num::{NonZeroU8, NonZeroU32};

use campfire_capabilities::Release;
use campfire_codec::{Toml, TomlTable};
use campfire_common::DeclaredName;
use campfire_declare::UnitTypeData;
use campfire_items::{InventoryData, ShopData};
use campfire_math::Num;
use campfire_package::{
    ContentError, ItemProblem, LoadError, LoadProblem, ModePackages, PackageRef, Place,
};
use campfire_script::NameKind;
use campfire_sim::Capability;
use campfire_test_packages::Edit;

use crate::package::moba::{edited, load};

const MANIFEST: &str = "modes/3v3/manifest.toml";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";
const HUSK: &str = "heroes/husk/data/avatar.toml";
const UNITS: &str = "modes/3v3/data/units.toml";

/// The 3v3 with `more` edits.
fn with_items(more: &[(&'static str, Edit<'static>)]) -> Result<ModePackages, LoadError> {
    load(&edited(more.iter().copied()))
}

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

#[test]
fn the_3v3s_items_read_as_their_data_holds_them() {
    let packages = with_items(&[]).unwrap();
    let items = &packages.content().items;
    assert_eq!(items.len(), 11);
    // A health potion of 50, five to a slot, of one use, drunk by its action.
    let potion = &items[&name("health_potion")];
    assert_eq!(potion.cost[&name("gold")], 50);
    assert_eq!(
        (potion.stack.get(), potion.uses.map(NonZeroU32::get)),
        (5, Some(1))
    );
    assert_eq!(potion.action, Some(name("drink_health_potion")));
    // A long sword: a stack of one by default, no uses, its stat its modifier.
    let sword = &items[&name("long_sword")];
    assert_eq!((sword.stack.get(), sword.uses), (1, None));
    assert_eq!(sword.modifiers, [name("long_sword")]);
    // A warblade, built from a long sword and a ruby crystal, 350 + 400 of its 1100.
    let warblade = &items[&name("warblade")];
    assert_eq!(
        warblade.components,
        [name("long_sword"), name("ruby_crystal")]
    );
    assert_eq!(warblade.cost[&name("gold")], 1100);
    // The fountain sells the 11 for gold, to its allies within 8 m.
    let fountain = &packages.content().units[&name("fountain")];
    let shop = fountain.section::<ShopData>().unwrap();
    assert_eq!(shop.items.len(), 11);
    assert_eq!(shop.resource.as_str(), "gold");
    assert_eq!(shop.reach, Num::int(8));
    assert_eq!(shop.buyers.to_string(), "allies");
    // 70% of a warblade's 1100 is 770.
    assert_eq!(shop.sell_share.of(1100), 770);
    // A unit type's inventory, as every hero's reads.
    let raw = Toml::parse::<TomlTable>("inventory = { slots = 6, kind = \"item\" }").unwrap();
    let unit: UnitTypeData = raw
        .read_with(UnitTypeData::seed(&Release::sections()))
        .unwrap();
    let inventory = unit.section::<InventoryData>().unwrap();
    assert_eq!(
        (inventory.slots, &inventory.kind),
        (NonZeroU8::new(6).unwrap(), &name("item"))
    );
}

/// A flaw of the items, as edits to the 3v3 with items, the package it fails, and its problem.
#[derive(Debug)]
struct Flaw {
    edits: &'static [(&'static str, Edit<'static>)],
    package: &'static str,
    refused: fn(&LoadProblem) -> bool,
}

const MODE: &str = "moba-3v3";

/// Whether `problem` is `name` of `of` unknown at `at`.
fn unknown(problem: &LoadProblem, at: &Place, of: NameKind, name: &str) -> bool {
    matches!(problem, LoadProblem::Unknown { at: found_at, name: found, of: kind } if found_at == at && *kind == of && found == name)
}

/// Whether `problem` is `file` failing to read with a message that has `message`.
fn unread(problem: &LoadProblem, file: &str, message: &str) -> bool {
    matches!(problem, LoadProblem::Content(ContentError::Data { path, error }) if path.to_string() == file && error.message().contains(message))
}

fn item(id: &str) -> Place {
    Place::Item(name(id))
}

/// The 3v3's fountain, its shop type.
fn fountain() -> Place {
    Place::UnitType(name("fountain"))
}

const FLAWS: [Flaw; 23] = [
    // An item has a name, a message of the mode's own language.
    Flaw {
        edits: &[(MODE_DATA, Edit::Remove("items.health_potion.name"))],
        package: MODE,
        refused: |problem| unread(problem, "data/mode.toml", "missing field `name`"),
    },
    Flaw {
        edits: &[(
            "modes/3v3/locale/en.ftl",
            Edit::Replace("item-health-potion = Health Potion\n", ""),
        )],
        package: MODE,
        refused: |problem| {
            unknown(
                problem,
                &item("health_potion"),
                NameKind::Message,
                "item-health-potion",
            )
        },
    },
    // An item costs in the mode's player resources.
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.long_sword.cost", "{ silver = 350 }"),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &item("long_sword"), NameKind::Resource, "silver"),
    },
    // An item is built from the mode's items, never from itself.
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.warblade.components", r#"["sword"]"#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &item("warblade"), NameKind::Item, "sword"),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.long_sword.components", r#"["warblade"]"#),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ComponentLoop(item)) if item == "long_sword"),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.long_sword.components", r#"["long_sword"]"#),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ComponentLoop(item)) if item == "long_sword"),
    },
    // An item costs at least what its components cost: 350 + 400 = 750.
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.warblade.cost", "{ gold = 749 }"),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::CheaperThanComponents(item)) if item == "warblade"),
    },
    // A full stack's cost is what an amount holds: (2³² − 1)², about 1.8 · 10¹⁹, is past 2⁶³ − 1.
    Flaw {
        edits: &[
            (
                MODE_DATA,
                Edit::Set("items.health_potion.stack", "4294967295"),
            ),
            (
                MODE_DATA,
                Edit::Set("items.health_potion.cost", "{ gold = 4294967295 }"),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::StackCost(item)) if item == "health_potion"),
    },
    // A stack and uses of at least one.
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.health_potion.stack", "0"))],
        package: MODE,
        refused: |problem| unread(problem, "data/mode.toml", "nonzero"),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.health_potion.uses", "0"))],
        package: MODE,
        refused: |problem| unread(problem, "data/mode.toml", "nonzero"),
    },
    // An item's modifiers and action are the mode's.
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.warblade.modifiers", r#"["sharp"]"#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &item("warblade"), NameKind::Modifier, "sharp"),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.health_potion.action", r#""sip""#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &item("health_potion"), NameKind::Ability, "sip"),
    },
    // An inventory fills a slot kind of the mode of one rank, with a slot at least, that no unit
    // type or choice puts other actions in.
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.kind", r#""bag""#))],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Unknown { of: NameKind::SlotKind, name, .. } if name == "bag"),
    },
    Flaw {
        edits: &[
            (
                MODE_DATA,
                Edit::Replace(
                    "[[slots]]\nname = \"item\"\n",
                    "[[slots]]\nname = \"item\"\n\n[[slots]]\nname = \"relics\"\nranks = 2\n",
                ),
            ),
            (HUSK, Edit::Set("inventory.kind", r#""relics""#)),
        ],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::RankedInventory { kind, .. }) if kind == "relics"),
    },
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.kind", r#""spell""#))],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::InventoryKindSlotted { at: Place::Choice(choice), kind }) if choice == "spells" && kind == "spell"),
    },
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.slots", "0"))],
        package: "hero-husk",
        refused: |problem| unread(problem, "data/avatar.toml", "nonzero"),
    },
    // A shop sells the mode's items, each only for its resource, to the units a filter of the
    // match's tags selects, within a reach that is not negative, giving back a share of 0 to 1.
    Flaw {
        edits: &[(
            UNITS,
            Edit::Set("units.fountain.shop.items", r#"["health_potion", "sword"]"#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &fountain(), NameKind::Item, "sword"),
    },
    Flaw {
        edits: &[(
            UNITS,
            Edit::Set("units.fountain.shop.resource", r#""silver""#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &fountain(), NameKind::Resource, "silver"),
    },
    Flaw {
        edits: &[(
            UNITS,
            Edit::Set("units.fountain.shop.buyers", r#""allies:shopper""#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &fountain(), NameKind::Filter, "allies:shopper"),
    },
    Flaw {
        edits: &[
            (MODE_DATA, Edit::Set("resources", r#"["gold", "silver"]"#)),
            (
                MODE_DATA,
                Edit::Set("items.health_potion.cost", "{ gold = 50, silver = 1 }"),
            ),
        ],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ShopResource { at, item }) if *at == fountain() && item == "health_potion"),
    },
    Flaw {
        edits: &[(UNITS, Edit::Set("units.fountain.shop.reach", r#""-1.0""#))],
        package: MODE,
        refused: |problem| {
            unread(
                problem,
                "data/units.toml",
                "a shop's reach is a number that is not negative",
            )
        },
    },
    Flaw {
        edits: &[(
            UNITS,
            Edit::Set("units.fountain.shop.sell_share", r#""1.5""#),
        )],
        package: MODE,
        refused: |problem| {
            unread(
                problem,
                "data/units.toml",
                "a share is 0, 1, or a decimal from 0 to 1",
            )
        },
    },
    // Items need `items`.
    Flaw {
        edits: &[(MANIFEST, Edit::Replace(r#", "items"]"#, "]"))],
        package: MODE,
        refused: |problem| {
            matches!(
                problem,
                LoadProblem::Undeclared {
                    capability: Capability::Items,
                    ..
                }
            )
        },
    },
];

#[test]
fn every_flaw_of_the_items_fails_the_load_with_its_own_problem() {
    for flaw in &FLAWS {
        let Err(error) = with_items(flaw.edits) else {
            panic!("{flaw:?} loads");
        };
        let LoadError { package, problem } = &error;
        let named = matches!(package, PackageRef::Name(name) if name == flaw.package);
        assert!(named && (flaw.refused)(problem), "{flaw:?}: {error:?}");
    }
    // An avatar holds no item types.
    let carried = [(
        HUSK,
        Edit::Set(
            "items.charm",
            r#"{ name = "item-charm", cost = { gold = 10 } }"#,
        ),
    )];
    let error = with_items(&carried).unwrap_err();
    assert!(
        matches!(*error.problem, LoadProblem::Item(ItemProblem::OutsideMode)),
        "{error:?}"
    );
}
