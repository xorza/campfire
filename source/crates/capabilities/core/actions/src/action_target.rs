use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::{Position, SimWorld, StableId};
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an action is aimed at: nothing, a unit, or a point, which an action that aims at a
/// direction aims through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ActionTarget<R: OrderRefs = Ids> {
    None,
    Unit(R::Unit),
    Point(R::Point),
}

impl Copy for ActionTarget {}

impl<R: OrderRefs> ActionTarget<R> {
    /// The same target with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ActionTarget<T>, M::Error> {
        Ok(match self {
            Self::None => ActionTarget::None,
            Self::Unit(unit) => ActionTarget::Unit(refs.aimed(unit)?),
            Self::Point(point) => ActionTarget::Point(refs.point(point)?),
        })
    }
}

impl ActionTarget {
    /// The point it names in `world`: its point, or where its unit stands; `None` for no
    /// target, or a unit that is gone.
    pub fn position(self, world: &World) -> Option<Vec3> {
        match self {
            Self::None => None,
            Self::Point(at) => Some(at),
            Self::Unit(id) => {
                let entity = world.entity_of(id)?;
                world.get::<Position>(entity).map(|position| position.get())
            }
        }
    }

    /// Whether its point, when it is one, lies within the world's bound, as a restore checks it.
    pub(crate) const fn within_bound(self) -> bool {
        match self {
            Self::Point(at) => at.within_bound(),
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
