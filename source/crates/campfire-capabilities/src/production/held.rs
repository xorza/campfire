use campfire_common::PlayerSlot;

use crate::units::unit_type::UnitType;

/// A living, complete unit a player owns, by its type, for a requirement to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Held {
    pub(crate) owner: PlayerSlot,
    pub(crate) unit_type: UnitType,
}

impl Held {
    /// Fills `held` with `units`, each once, in order.
    pub(crate) fn collect(held: &mut Vec<Held>, units: impl IntoIterator<Item = Held>) {
        held.clear();
        held.extend(units);
        held.sort_unstable();
        held.dedup();
    }
}
