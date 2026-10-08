use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::{Position, StableId};

use crate::deliveries::delivering::Delivering;
use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::projectiles::Projectiles;
use crate::projectiles::projectile::Flight;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::units::unit_type::UnitType;
use campfire_math::Num;

/// A projectile a call queued: `id`, the id the call took for it, of `by`, of `unit_type`, from
/// `from`, toward a direction or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectilesEffect {
    pub(crate) id: StableId,
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
    fn apply(self, world: &mut World, _: &mut Frame, _: Tick) {
        let from = world.resource::<Bounds>().clamp(self.from);
        let flight = match self.toward {
            Toward::Unit(target) => Flight::Homing {
                target,
                flown: Num::ZERO,
                lost: false,
            },
            Toward::Direction(direction) => {
                let Some(direction) = world.resource::<Metric>().direction(direction) else {
                    return;
                };
                let range = Projectiles::range(world, self.by, self.unit_type);
                Flight::Line {
                    direction,
                    flown: Num::ZERO,
                    range: Projectiles::reach(*world.resource::<Bounds>(), range, from, direction),
                    aimed: None,
                }
            }
        };
        let id = Some(self.id);
        Projectiles::push(world, self.by, self.unit_type, from, id, [flight]);
    }
}
