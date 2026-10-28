use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;
use campfire_math::Vec3;
use campfire_sim::StableId;

use crate::build_target::BuildTarget;

/// An order given to a unit: `orders`, which gives every order of every source, triggers it once
/// it gave the order, and each capability that keeps a loop of the unit's sets or ends its own
/// in its observer, so `orders` writes no part of theirs.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitOrdered {
    pub entity: Entity,
    pub order: OrderKind,
}

/// What an order asks of the capabilities that keep a unit's loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderKind {
    /// Build with the build in `slot` at `target`, its point within the map's bounds.
    Build { slot: u8, target: BuildTarget },
    /// Gather with the gather in `slot` at the node `node`, aimed at it from `from`, where the
    /// unit stands.
    Gather {
        slot: u8,
        node: StableId,
        from: Vec3,
    },
    /// Use the object `target`, with the use slot and the action its users give the unit.
    Use { target: StableId },
    /// Enter the container `target`, with the use slot and its entry
    /// (04-capabilities/interaction.md, An entry is an action of the container's).
    Enter { target: StableId },
    /// Let `rider` out of the unit, a container, which keeps every loop of its own.
    Exit { rider: StableId },
    /// Let every rider out of the unit, a container, which keeps every loop of its own.
    Evacuate,
    /// Attack a unit, or move attacking on the way, which ends every loop, as any other order
    /// does, and lets an assault transport's riders out to fight (exits-docks-and-selection.md,
    /// A transport may assault).
    Attack,
    /// Any other order, which ends every loop.
    Other,
}
