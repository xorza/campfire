use campfire_actions::ActionTarget;
use campfire_players::CapabilityAction;
use campfire_sim::Capability;
use campfire_units::{BuildTarget, Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `orders` tells each of its units to do, as the unit's own order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum OrdersAction<R: OrderRefs = Ids> {
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
}

impl Copy for OrdersAction {}

impl CapabilityAction for OrdersAction {
    const CAPABILITY: Capability = Capability::Orders;
}

impl<R: OrderRefs> OrdersAction<R> {
    /// The same action with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<OrdersAction<T>, M::Error> {
        Ok(match self {
            Self::Move { to } => OrdersAction::Move {
                to: refs.ground(to)?,
            },
            Self::Attack { target } => OrdersAction::Attack {
                target: refs.unit(target)?,
            },
            Self::Slot { slot, target } => OrdersAction::Slot {
                slot: refs.slot(slot)?,
                target: target.map_refs(refs)?,
            },
            Self::Build { slot, target } => OrdersAction::Build {
                slot: refs.slot(slot)?,
                target: target.map_refs(refs)?,
            },
            Self::Stop => OrdersAction::Stop,
            Self::Use { target } => OrdersAction::Use {
                target: refs.unit(target)?,
            },
        })
    }
}
