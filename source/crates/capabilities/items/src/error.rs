use campfire_common::DeclaredName;
use campfire_declarations::BookPlace;
use thiserror::Error;

use campfire_values::ItemId;

/// Why the mode's item types make no item book. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ItemError {
    /// A full stack of the item costs, in some resource, more than an amount holds: what a sale
    /// of the stack gives back.
    #[error("a full stack costs more than an amount holds")]
    StackCost(ItemId),
    /// The item costs less, in some resource, than the components it is built from, which a
    /// purchase takes off its price.
    #[error("it costs less than its components")]
    CheaperThanComponents(ItemId),
}

/// Why the packages' item types, an inventory or a shop break a rule of items. Packages are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ItemsDataError {
    /// A package other than the mode holds item types, which no shop or inventory names.
    #[error("only the mode package holds item types")]
    OutsideMode,
    /// The item is built, through its components, from itself.
    #[error("built from itself through its components")]
    ComponentLoop,
    /// The item holds `modifier`, the passive of `holder`, which would share its hold.
    #[error("holds modifier {modifier}, the passive of {holder}")]
    SharedPassive {
        modifier: DeclaredName,
        holder: BookPlace,
    },
    /// The item's action is a use, which only its object runs.
    #[error("its action {0} is a use, which only its object runs")]
    UseAction(DeclaredName),
    /// The shop sells the item, which costs in a resource the shop does not take.
    #[error("sells item {0}, which costs in another resource")]
    ShopResource(DeclaredName),
}
