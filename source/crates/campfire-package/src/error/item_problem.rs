use std::fmt;

use campfire_capabilities::{ActionSlots, DeclaredName};

use crate::error::place::Place;

/// What is wrong with the mode's item types, its shop or an inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemProblem {
    /// A package other than the mode holds item types, which no shop or inventory names.
    OutsideMode,
    /// The item is built, through its components, from itself.
    ComponentLoop(DeclaredName),
    /// An inventory at `at` fills a slot kind with ranks, while an item's action has one.
    RankedInventory { at: Place, kind: DeclaredName },
    /// A unit type at `at` holds more slots, its own and its inventory's, than a unit holds.
    TooManySlots(Place),
    /// The item costs less, in some resource, than the components it is built from.
    CheaperThanComponents(DeclaredName),
    /// The shop sells the item, which costs in a resource the shop does not take.
    ShopResource(DeclaredName),
    /// The marker has the shop's tag, and no region or no team to serve.
    ShopMarker(DeclaredName),
    /// A unit type or a choice at `at` puts actions in `kind`, whose slots an inventory fills.
    InventoryKindSlotted { at: Place, kind: DeclaredName },
}

impl fmt::Display for ItemProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemProblem::OutsideMode => f.write_str("only the mode package holds item types"),
            ItemProblem::ComponentLoop(item) => {
                write!(f, "item {item} is built from itself through its components")
            }
            ItemProblem::RankedInventory { at, kind } => {
                write!(
                    f,
                    "{at}: its inventory fills {kind}, a slot kind with ranks"
                )
            }
            ItemProblem::CheaperThanComponents(item) => {
                write!(f, "item {item} costs less than its components")
            }
            ItemProblem::ShopResource(item) => {
                write!(
                    f,
                    "the shop sells item {item}, which costs in another resource"
                )
            }
            ItemProblem::ShopMarker(marker) => {
                write!(
                    f,
                    "marker {marker} has the shop's tag, and no region or no team"
                )
            }
            ItemProblem::InventoryKindSlotted { at, kind } => {
                write!(
                    f,
                    "{at}: puts actions in {kind}, whose slots an inventory fills"
                )
            }
            ItemProblem::TooManySlots(at) => write!(
                f,
                "{at}: more than {} slots with its inventory",
                ActionSlots::LIMIT
            ),
        }
    }
}
