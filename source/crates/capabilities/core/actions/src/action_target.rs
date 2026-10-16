use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::{Position, SimWorld, StableId};
use serde::{Deserialize, Serialize};

/// What an action is aimed at: nothing, a unit, or a point, which an action that aims at a
/// direction aims through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionTarget {
    None,
    Unit(StableId),
    Point(Vec3),
}
impl ActionTarget {
    /// The point it names in `world`: its point, or where its unit stands; `None` for no
    /// target, or a unit that is gone.
    pub fn pos(self, world: &World) -> Option<Vec3> {
        match self {
            Self::None => None,
            Self::Point(at) => Some(at),
            Self::Unit(id) => {
                let entity = world.entity_of(id)?;
                world.get::<Position>(entity).map(|pos| pos.get())
            }
        }
    }

    /// Whether each of its places lies within the world's bound, as a restore checks it.
    pub(crate) fn within_bound(self) -> bool {
        match self {
            Self::Point(at) => Position::from(at).within_bound(),
            Self::None | Self::Unit(_) => true,
        }
    }

    pub const fn unit(self) -> Option<StableId> {
        match self {
            Self::Unit(unit) => Some(unit),
            Self::None | Self::Point(_) => None,
        }
    }
}

impl From<StableId> for ActionTarget {
    fn from(unit: StableId) -> Self {
        Self::Unit(unit)
    }
}

impl From<Vec3> for ActionTarget {
    fn from(point: Vec3) -> Self {
        Self::Point(point)
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_unit_or_a_point_is_its_target() {
        let unit = IdAllocator::default().allocate();
        let point = Vec3::from((3, 0, -4));
        assert_eq!(ActionTarget::from(unit), ActionTarget::Unit(unit));
        assert_eq!(ActionTarget::from(point), ActionTarget::Point(point));
    }
}
