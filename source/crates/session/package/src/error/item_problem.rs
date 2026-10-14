use campfire_actions::ActionSlots;
use campfire_values::DeclaredName;
use thiserror::Error;

use crate::error::place::Place;

/// What is wrong with the mode's item types, a shop or an inventory.
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
    /// The shop type at `at` sells the item, which costs in a resource the shop does not take.
    #[error("{at}: sells item {item}, which costs in another resource")]
    ShopResource { at: Place, item: DeclaredName },
    /// A unit type or a choice at `at` puts actions in `kind`, whose slots an inventory fills.
    #[error("{at}: puts actions in {kind}, whose slots an inventory fills")]
    InventoryKindSlotted { at: Place, kind: DeclaredName },
}
