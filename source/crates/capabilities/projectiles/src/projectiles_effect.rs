use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_deliveries::Delivering;
use campfire_math::Vec3;
use campfire_script::{Effect, EffectKind, Frame};
use campfire_sim::StableId;
use campfire_units::MapBook;
use campfire_values::UnitType;

use crate::Projectiles;
use crate::projectile::{Flight, LaunchAim};
use crate::projectile_flight::ProjectileFlight;
use crate::projectile_spec::ProjectileSpec;

/// A projectile a call queued: `id`, the id the call took for it, of `by`, of `unit_type`, from
/// `from`, toward a direction or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectilesEffect {
    pub id: StableId,
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub(crate) from: Vec3,
    pub(crate) toward: Toward,
}

/// Where a projectile flies: along a direction, or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toward {
    Direction(Vec3),
    Point(Vec3),
    Unit(StableId),
}

impl Effect for ProjectilesEffect {
    const KIND: EffectKind = EffectKind::Change;

    /// Applies the effect: a launch this tick, its own cast, from the point of the map's bounds
    /// nearest where it says, homing on its unit, on a curve or steered to its unit's center or
    /// to its point for a type that flies one, or along its direction. A direction of no length,
    /// or a curve's or a steer's unit that is gone, launches nothing.
    fn apply(self, world: &mut World, _: &mut Frame) {
        let from = world.resource::<MapBook>().bounds.clamp(self.from);
        let spec = world
            .resource::<ById<UnitType, ProjectileSpec>>()
            .get(self.unit_type)
            .expect("a delivery's projectile type has a spec");
        let flight = match self.toward {
            Toward::Unit(target)
                if matches!(
                    spec.flight(),
                    ProjectileFlight::Curve | ProjectileFlight::Steered
                ) =>
            {
                let Some(to) = Projectiles::center(world, target) else {
                    return;
                };
                let map = world.resource::<MapBook>();
                let aim = LaunchAim {
                    from: map.point(from),
                    to,
                    aimed: Some(target),
                };
                Flight::aimed(aim, spec, map)
            }
            Toward::Point(to) => {
                let map = world.resource::<MapBook>();
                let aim = LaunchAim {
                    from: map.point(from),
                    to: map.point(map.bounds.clamp(to)),
                    aimed: None,
                };
                Flight::aimed(aim, spec, map)
            }
            Toward::Unit(target) => Flight::homing(target),
            Toward::Direction(direction) => {
                let Some(direction) = world.resource::<MapBook>().metric().direction(direction)
                else {
                    return;
                };
                let range = Projectiles::line_range(world, self.by, self.unit_type);
                let bounds = world.resource::<MapBook>().bounds;
                Flight::line(
                    from,
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
