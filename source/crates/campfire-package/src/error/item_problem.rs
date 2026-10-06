use campfire_capabilities::{ActionSlots, DeclaredName};
use thiserror::Error;

use crate::error::place::Place;

/// What is wrong with the mode's item types, its shop or an inventory.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ItemProblem {
    /// A package other than the mode holds item types, which no shop or inventory names.
    #[error("only the mode package holds item types")]
    OutsideMode,
    /// The item is built, through its components, from itself.
    #[error("item {0} is built from itself through its components")]
    ComponentLoop(DeclaredName),
    /// An inventory at `at` fills a slot kind with ranks, while an item's action has one.
    #[error("{at}: its inventory fills {kind}, a slot kind with ranks")]
    RankedInventory { at: Place, kind: DeclaredName },
    /// A unit type at `at` holds more slots, its own and its inventory's, than a unit holds.
    #[error("{0}: more than {limit} slots with its inventory", limit = ActionSlots::LIMIT)]
    TooManySlots(Place),
    /// The item costs less, in some resource, than the components it is built from.
    #[error("item {0} costs less than its components")]
    CheaperThanComponents(DeclaredName),
    /// The shop sells the item, which costs in a resource the shop does not take.
    #[error("the shop sells item {0}, which costs in another resource")]
    ShopResource(DeclaredName),
    /// The marker has the shop's tag, and no region or no team to serve.
    #[error("marker {0} has the shop's tag, and no region or no team")]
    ShopMarker(DeclaredName),
    /// A unit type or a choice at `at` puts actions in `kind`, whose slots an inventory fills.
    #[error("{at}: puts actions in {kind}, whose slots an inventory fills")]
    InventoryKindSlotted { at: Place, kind: DeclaredName },
}
