use campfire_actions::ActionTarget;
use campfire_players::CapabilityDirective;
use campfire_sim::Capability;
use campfire_units::{BuildTarget, Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `orders` tells each of its units to do, as the unit's own order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum OrdersDirective<R: OrderRefs = Ids> {
    /// Walk to a point on the ground plane; a group keeps its shape on a move away from it.
    Move { to: R::Ground },
    /// Attack a unit until it dies or another order comes.
    Attack { target: R::Unit },
    /// Start the action in `slot` at `target`: a cast, a train, or a gather.
    Slot {
        slot: R::Slot,
        target: ActionTarget<R>,
    },
    /// Build with the build in `slot` at `target`.
    Build {
        slot: R::Slot,
        target: BuildTarget<R>,
    },
    /// End what is under way, drop the target and the destination, and stand.
    Stop,
    /// Use the object `target` with the use slot: its action runs from that slot.
    Use { target: R::Unit },
    /// Enter the container `target` with the use slot: its entry runs from that slot
    /// (04-capabilities/interaction.md, An entry is an action of the container's).
    Enter { target: R::Unit },
    /// Let `rider` out of the container the order's unit is, after the riders before it
    /// (04-capabilities/interaction.md, A rider leaves at a free cell by the container).
    Exit { rider: R::Unit },
    /// Let every rider out of the container the order's unit is, in their order.
    Evacuate,
    /// Walk to a point on the ground plane as a move does, attacking each enemy the scan finds on
    /// the way, then walking on (04-capabilities/control.md, An army attack-moves).
    AttackMove { to: R::Ground },
}

impl Copy for OrdersDirective {}

impl CapabilityDirective for OrdersDirective {
    const CAPABILITY: Capability = Capability::Orders;
}

impl<R: OrderRefs> OrdersDirective<R> {
    /// The same directive with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<OrdersDirective<T>, M::Error> {
        Ok(match self {
            Self::Move { to } => OrdersDirective::Move {
                to: refs.ground(to)?,
            },
            Self::Attack { target } => OrdersDirective::Attack {
                target: refs.attack(target)?,
            },
            Self::AttackMove { to } => OrdersDirective::AttackMove {
                to: refs.ground(to)?,
            },
            Self::Slot { slot, target } => OrdersDirective::Slot {
                slot: refs.slot(slot)?,
                target: target.map_refs(refs)?,
            },
            Self::Build { slot, target } => OrdersDirective::Build {
                slot: refs.slot(slot)?,
                target: target.map_refs(refs)?,
            },
            Self::Stop => OrdersDirective::Stop,
            Self::Use { target } => OrdersDirective::Use {
                target: refs.used(target)?,
            },
            Self::Enter { target } => OrdersDirective::Enter {
                target: refs.entered(target)?,
            },
            Self::Exit { rider } => OrdersDirective::Exit {
                rider: refs.unit(rider)?,
            },
            Self::Evacuate => OrdersDirective::Evacuate,
        })
    }
}
