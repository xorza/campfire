use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, Position, StableId};
use serde::{Deserialize, Serialize};

/// What an action is aimed at: nothing, a unit, or a point, which an action that aims at a
/// direction aims through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionTarget {
    None,
    Unit(StableId),
    Point(Position),
}
impl ActionTarget {
    /// The point it names in `world`: its point, or where its unit stands; `None` for no
    /// target, or a unit that is gone.
    pub(crate) fn point(self, world: &World) -> Option<Position> {
        match self {
            ActionTarget::None => None,
            ActionTarget::Point(at) => Some(at),
            ActionTarget::Unit(unit) => {
                let entity = world.resource::<EntityIndex>().get(unit)?;
                world.get::<Position>(entity).copied()
            }
        }
    }

    pub(crate) const fn unit(self) -> Option<StableId> {
        match self {
            ActionTarget::Unit(unit) => Some(unit),
            ActionTarget::None | ActionTarget::Point(_) => None,
        }
    }
}
