//! The 3v3's item types, its shop and its heroes' inventories: each reads as its data holds it,
//! and each flaw of them fails the load with its own problem.

use std::num::{NonZeroU8, NonZeroU32};

use campfire_capabilities::{DeclaredName, NameKind, UnitTypeFile};
use campfire_common::Toml;
use campfire_package::{
    ChoiceProblem, ContentError, ItemProblem, LoadError, LoadProblem, ModePackages, PackageRef,
    Place,
};
use campfire_sim::Capability;

use crate::moba::{Edit, edited, load};

const MANIFEST: &str = "modes/3v3/manifest.toml";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";
const HUSK: &str = "heroes/husk/data/avatar.toml";
const MAP: &str = "modes/3v3/map/two_lanes/map.toml";

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
    let shop = packages.data().shop.as_ref().unwrap();
    assert_eq!(shop.items.len(), 11);
    assert_eq!((shop.resource.as_str(), shop.at.as_str()), ("gold", "shop"));
    // 70% of a warblade's 1100 is 770.
    assert_eq!(shop.sell_share.of(1100), 770);
    // A unit type's inventory, as every hero's reads.
    let unit: UnitTypeFile = Toml::parse("inventory = { slots = 6, kind = \"item\" }").unwrap();
    let inventory = unit.inventory.unwrap();
    assert_eq!(
        (inventory.slots, inventory.kind),
        (NonZeroU8::new(6).unwrap(), name("item"))
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

/// Whether `problem` is `name` of `of` unknown at `place`.
fn unknown(problem: &LoadProblem, place: &Place, of: NameKind, name: &str) -> bool {
    matches!(problem, LoadProblem::Unknown { at, name: found, of: kind } if at == place && *kind == of && found == name)
}

/// Whether `problem` is `file` failing to read with a message that has `message`.
fn unread(problem: &LoadProblem, file: &str, message: &str) -> bool {
    matches!(problem, LoadProblem::Content(ContentError::Data { path, error }) if path.to_string() == file && error.message().contains(message))
}

fn item(id: &str) -> Place {
    Place::Item(name(id))
}

const FLAWS: [Flaw; 20] = [
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
        refused: |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownSlotKind { kind, .. }) if kind == "bag"),
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
    // The shop sells the mode's items, each only for its resource, at a tag of the map's markers,
    // each with a region and a team, giving back a share of 0 to 1.
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("shop.items", r#"["health_potion", "sword"]"#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &Place::Shop, NameKind::Item, "sword"),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("shop.resource", r#""silver""#))],
        package: MODE,
        refused: |problem| unknown(problem, &Place::Shop, NameKind::Resource, "silver"),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("shop.at", r#""base""#))],
        package: MODE,
        refused: |problem| unknown(problem, &Place::Shop, NameKind::MarkerTag, "base"),
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
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ShopResource(item)) if item == "health_potion"),
    },
    Flaw {
        edits: &[(MAP, Edit::Remove("markers.2.region"))],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ShopMarker(marker)) if marker == "north_shop"),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("shop.sell_share", r#""1.5""#))],
        package: MODE,
        refused: |problem| {
            unread(
                problem,
                "data/mode.toml",
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
    let carried = [(HUSK, Edit::Set("items.charm", "{ cost = { gold = 10 } }"))];
    let error = with_items(&carried).unwrap_err();
    assert!(
        matches!(*error.problem, LoadProblem::Item(ItemProblem::OutsideMode)),
        "{error:?}"
    );
}
