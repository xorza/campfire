use bevy_ecs::world::World;
use campfire_deliveries::Delivering;
use campfire_math::Vec3;
use campfire_script::{Effect, Frame};
use campfire_sim::{Position, StableId};
use campfire_units::MapBook;
use campfire_values::UnitType;

use crate::Projectiles;
use crate::projectile::Flight;

/// A projectile a call queued: `id`, the id the call took for it, of `by`, of `unit_type`, from
/// `from`, toward a direction or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectilesEffect {
    pub id: StableId,
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub(crate) from: Position,
    pub(crate) toward: Toward,
}

/// Where a projectile flies: along a direction, or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toward {
    Direction(Vec3),
    Unit(StableId),
}

impl Effect for ProjectilesEffect {
    /// Applies the effect: a launch this tick, its own cast, from the point of the map's bounds
    /// nearest where it says. A direction of no length launches nothing.
    fn apply(self, world: &mut World, _: &mut Frame) {
        let from = world.resource::<MapBook>().bounds.clamp(self.from);
        let flight = match self.toward {
            Toward::Unit(target) => Flight::homing(target),
            Toward::Direction(direction) => {
                let Some(direction) = world.resource::<MapBook>().metric.direction(direction)
                else {
                    return;
                };
                let range = Projectiles::line_range(world, self.by, self.unit_type);
                let bounds = world.resource::<MapBook>().bounds;
                Flight::line(
                    direction,
                    Projectiles::reach(bounds, range, from, direction),
                    None,
                )
            }
        };
        let id = Some(self.id);
        Projectiles::push(world, self.by, self.unit_type, from, id, [flight]);
    }
}
