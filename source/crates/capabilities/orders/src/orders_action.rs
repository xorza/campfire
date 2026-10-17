use campfire_actions::ActionTarget;
use campfire_math::Num;
use campfire_players::CapabilityAction;
use campfire_sim::{Capability, StableId};
use campfire_units::BuildTarget;
use serde::{Deserialize, Serialize};

/// What an order of `orders` tells each of its units to do, as the unit's own order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrdersAction {
    /// Walk to a point on the ground plane; a group keeps its shape on a move away from it.
    Move { x: Num, z: Num },
    /// Attack a unit until it dies or another order comes.
    Attack { target: StableId },
    /// Start the action in `slot` at `target`: a cast, a train, or a gather.
    Slot { slot: u8, target: ActionTarget },
    /// Build with the build in `slot` at `target`.
    Build { slot: u8, target: BuildTarget },
    /// End what is under way, drop the target and the destination, and stand.
    Stop,
    /// Use the object `target` with the use slot: its action runs from that slot.
    Use { target: StableId },
}

impl CapabilityAction for OrdersAction {
    const CAPABILITY: Capability = Capability::Orders;
}
