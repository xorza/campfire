use bevy_ecs::world::World;
use campfire_sim::{Position, SimWorld, StableId};
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
    pub fn point(self, world: &World) -> Option<Position> {
        match self {
            ActionTarget::None => None,
            ActionTarget::Point(at) => Some(at),
            ActionTarget::Unit(id) => {
                let entity = world.entity_of(id)?;
                world.get::<Position>(entity).copied()
            }
        }
    }

    pub const fn unit(self) -> Option<StableId> {
        match self {
            ActionTarget::Unit(unit) => Some(unit),
            ActionTarget::None | ActionTarget::Point(_) => None,
        }
    }
}

impl From<StableId> for ActionTarget {
    fn from(unit: StableId) -> ActionTarget {
        ActionTarget::Unit(unit)
    }
}

impl From<Position> for ActionTarget {
    fn from(point: Position) -> ActionTarget {
        ActionTarget::Point(point)
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_unit_or_a_point_is_its_target() {
        let unit = IdAllocator::default().allocate();
        let point = Position::from((3, 0, -4));
        assert_eq!(ActionTarget::from(unit), ActionTarget::Unit(unit));
        assert_eq!(ActionTarget::from(point), ActionTarget::Point(point));
    }
}
