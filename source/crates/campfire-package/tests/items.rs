//! The mode's item types, its shop and its units' inventories: each reads as its data holds it,
//! and each flaw of them fails the load with its own problem.

use std::num::{NonZeroU8, NonZeroU32};

use campfire_capabilities::{DeclaredName, NameKind, UnitTypeFile};
use campfire_math::Num;
use campfire_package::{
    ChoiceProblem, ContentError, ItemProblem, LoadError, LoadProblem, ModePackages, PackageRef,
    Place,
};
use campfire_sim::Capability;

use crate::moba::{Edit, edited};

const MANIFEST: &str = "modes/3v3/manifest.toml";
const MODE_DATA: &str = "modes/3v3/data/mode.toml";
const HUSK: &str = "heroes/husk/data/avatar.toml";

/// A potion of five to a slot that heals as it is drunk, a blade, and an edge built from the
/// blade, sold at the teams' spawns for gold, half back on a sale.
const ITEMS: &str = r#"[items.potion]
cost = { gold = 50 }
stack = 5
uses = 1
action = "drink"

[items.blade]
cost = { gold = 300 }

[items.edge]
cost = { gold = 500 }
components = ["blade"]
modifiers = ["warden_blessing"]

[actions.drink]
targeting = "none"
on_resolve = [{ heal = { amount = 50 }, to = "source" }]

[shop]
items = ["potion", "blade", "edge"]
resource = "gold"
at = "spawn"
sell_share = "0.5"

[actions.warden_attack]"#;

/// The 3v3 with `items` declared, the items above, a slot kind of items after the weapon, and
/// Husk's inventory of six of them; then `more` edits.
fn with_items(more: &[(&'static str, Edit<'static>)]) -> Result<ModePackages, LoadError> {
    let base = [
        (
            MANIFEST,
            Edit::Replace(r#""progression"]"#, r#""progression", "items"]"#),
        ),
        (
            MODE_DATA,
            Edit::Replace(
                "[[slots]]\nname = \"weapon\"\n",
                "[[slots]]\nname = \"weapon\"\n\n[[slots]]\nname = \"item\"\n",
            ),
        ),
        (MODE_DATA, Edit::Replace("[actions.warden_attack]", ITEMS)),
        (
            HUSK,
            Edit::Replace(
                "weapon = [\"attack\"] }\n",
                "weapon = [\"attack\"] }\ninventory = { slots = 6, kind = \"item\" }\n",
            ),
        ),
    ];
    ModePackages::from_package_dir(&edited(base.into_iter().chain(more.iter().copied())))
}

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

#[test]
fn items_read_as_their_data_holds_them() {
    let packages = with_items(&[]).unwrap();
    let items = &packages.content().items;
    let potion = &items[&name("potion")];
    assert_eq!(potion.cost[&name("gold")], 50);
    assert_eq!(
        (potion.stack.get(), potion.uses.map(NonZeroU32::get)),
        (5, Some(1))
    );
    assert_eq!(potion.action, Some(name("drink")));
    // A stack of one by default, no uses, no components and no modifiers.
    let blade = &items[&name("blade")];
    assert_eq!((blade.stack.get(), blade.uses), (1, None));
    let edge = &items[&name("edge")];
    assert_eq!(edge.components, [name("blade")]);
    assert_eq!(edge.modifiers, [name("warden_blessing")]);
    let shop = packages.data().shop.as_ref().unwrap();
    assert_eq!(shop.items, [name("potion"), name("blade"), name("edge")]);
    assert_eq!(
        (shop.resource.as_str(), shop.at.as_str()),
        ("gold", "spawn")
    );
    assert_eq!(shop.sell_share, Num::HALF);
    // A unit type's inventory, as Husk's reads.
    let unit: UnitTypeFile = toml::from_str("inventory = { slots = 6, kind = \"item\" }").unwrap();
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

const FLAWS: [Flaw; 16] = [
    // An item costs in the mode's player resources.
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.blade.cost", "{ silver = 300 }"))],
        package: MODE,
        refused: |problem| unknown(problem, &item("blade"), NameKind::Resource, "silver"),
    },
    // An item is built from the mode's items, never from itself.
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.edge.components", r#"["sword"]"#),
        )],
        package: MODE,
        refused: |problem| unknown(problem, &item("edge"), NameKind::Item, "sword"),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.blade.components", r#"["edge"]"#),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ComponentLoop(item)) if item == "blade"),
    },
    Flaw {
        edits: &[(
            MODE_DATA,
            Edit::Set("items.blade.components", r#"["blade"]"#),
        )],
        package: MODE,
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::ComponentLoop(item)) if item == "blade"),
    },
    // A stack and uses of at least one.
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.potion.stack", "0"))],
        package: MODE,
        refused: |problem| unread(problem, "data/mode.toml", "nonzero"),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.potion.uses", "0"))],
        package: MODE,
        refused: |problem| unread(problem, "data/mode.toml", "nonzero"),
    },
    // An item's modifiers and action are the mode's.
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.edge.modifiers", r#"["sharp"]"#))],
        package: MODE,
        refused: |problem| unknown(problem, &item("edge"), NameKind::Modifier, "sharp"),
    },
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("items.potion.action", r#""sip""#))],
        package: MODE,
        refused: |problem| unknown(problem, &item("potion"), NameKind::Ability, "sip"),
    },
    // An inventory fills a slot kind of the mode of one rank, with a slot at least.
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.kind", r#""bag""#))],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Choice(ChoiceProblem::UnknownSlotKind { kind, .. }) if kind == "bag"),
    },
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.kind", r#""basic""#))],
        package: "hero-husk",
        refused: |problem| matches!(problem, LoadProblem::Item(ItemProblem::RankedInventory { kind, .. }) if kind == "basic"),
    },
    Flaw {
        edits: &[(HUSK, Edit::Set("inventory.slots", "0"))],
        package: "hero-husk",
        refused: |problem| unread(problem, "data/avatar.toml", "nonzero"),
    },
    // The shop sells the mode's items for one of its resources at a tag of the map's markers,
    // giving back a share of 0 to 1.
    Flaw {
        edits: &[(MODE_DATA, Edit::Set("shop.items", r#"["potion", "sword"]"#))],
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
        edits: &[(MODE_DATA, Edit::Set("shop.sell_share", r#""1.5""#))],
        package: MODE,
        refused: |problem| {
            unread(
                problem,
                "data/mode.toml",
                "a sell share is a number from 0 to 1",
            )
        },
    },
    // Items need `items`, and only the mode holds them.
    Flaw {
        edits: &[(
            MANIFEST,
            Edit::Replace(r#""progression", "items"]"#, r#""progression"]"#),
        )],
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
        assert!(named && (flaw.refused)(problem), "{flaw:?}: {error}");
    }
    // An avatar holds no item types.
    let carried = [(HUSK, Edit::Set("items.charm", "{ cost = { gold = 10 } }"))];
    let error = with_items(&carried).unwrap_err();
    assert!(
        matches!(*error.problem, LoadProblem::Item(ItemProblem::OutsideMode)),
        "{error}"
    );
}
