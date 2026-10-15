use thiserror::Error;

use crate::item_id::ItemId;

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
