use campfire_common::DeclaredName;
use thiserror::Error;

use crate::error::place::Place;

/// What does not hold of the uses: a `use` action, a `use` section, or a `uses` section.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UseProblem {
    /// A use aims at something but a unit its filter selects.
    #[error("action \"{0}\": a use aims at a unit its filter selects")]
    Aims(DeclaredName),
    /// The `use` section at `at` names an action of another kind.
    #[error("{at}: its use names action \"{action}\", which is not of the `use` kind")]
    NotUse { at: Place, action: DeclaredName },
    /// Something at `at` other than a `use` section names a use: a unit type's slots, an item, or
    /// a loadout.
    #[error("{at}: names action \"{action}\", a use, which only a `use` section names")]
    Named { at: Place, action: DeclaredName },
    /// The `uses` at `at` names a slot kind with ranks, while a use's action has one.
    #[error("{at}: its uses name {kind}, a slot kind with ranks")]
    RankedKind { at: Place, kind: DeclaredName },
    /// The `uses` at `at` names a slot kind an inventory fills.
    #[error("{at}: its uses name {kind}, whose slots an inventory fills")]
    InventoryKind { at: Place, kind: DeclaredName },
    /// A unit type or a choice at `at` puts actions in `kind`, which holds the uses.
    #[error("{at}: puts actions in {kind}, which holds the uses")]
    KindSlotted { at: Place, kind: DeclaredName },
}
