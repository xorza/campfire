use campfire_common::{PlayerSlot, Tick};
use campfire_sim::{Position, StableId};
use serde::{Deserialize, Serialize};

/// The units that share one route search when they ask for routes in one tick: those one order
/// moves as a group, or those of one spawn group on their path, each with the goal of the group,
/// which the shared route goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Party {
    pub(crate) key: PartyKey,
    pub(crate) goal: Position,
}

/// Which group a party is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum PartyKey {
    /// The units of the order that player `slot` sent `number`th in tick `tick`.
    Order {
        tick: Tick,
        slot: PlayerSlot,
        number: u32,
    },
    /// The units of the spawn group whose first unit has this stable id.
    Spawn(StableId),
}
