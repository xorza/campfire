use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, Position, StableId};

use crate::actions::action::Aim;
use crate::actions::action_book::ActionBook;
use crate::actions::action_range::ActionRange;
use crate::actions::action_target::ActionTarget;
use crate::actions::delivery::{Delivery, DeliveryShape};
use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::deliveries::DeliverySet;
use crate::deliveries::deliverers::{Deliverers, DeliveryKind};
use crate::deliveries::delivering::Delivering;
use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::projectiles::flight_state::FlightState;
use crate::projectiles::flying::Flying;
use crate::projectiles::launches::{Launch, LaunchPayload, Launches};
use crate::projectiles::launching::Launching;
use crate::projectiles::projectile::{Flight, Projectile};
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::projectiles::projectiles_api::ProjectilesApi;
use crate::projectiles::struck_units::StruckUnits;
use crate::units::by_type::ByType;
use crate::units::engine_tag::EngineTag;
use crate::units::tag_properties::TagProperties;
use crate::units::unit_type::UnitType;

pub(crate) mod flight_state;
pub(crate) mod flights;
pub(crate) mod flying;
pub(crate) mod launches;
pub(crate) mod launching;
pub(crate) mod projectile;
pub(crate) mod projectile_data;
pub(crate) mod projectile_spec;
pub(crate) mod projectiles_api;
pub(crate) mod projectiles_effect;
pub(crate) mod struck_units;

/// The `projectiles` capability: projectile units, which ranged attacks fire and actions
/// deliver. It builds on combat, which a match installs too.
#[derive(Debug)]
pub struct Projectiles;

impl Module for Projectiles {
    const CAPABILITY: Option<Capability> = Some(Capability::Projectiles);
    const NEEDS: &'static [Capability] = &[Capability::Combat];

    fn declare<D: Declarations>(d: &mut D) {
        d.engine_tags(&[Projectiles::TAG]);
        d.component::<Projectile>();
        d.component::<FlightState>();
        d.resource::<StruckUnits>();
        d.scratch::<Launches>();
        d.callback::<Deliverers>(DeliveryKind::Projectile, Projectiles::deliver);
        d.require::<Projectile, FlightState>();
        d.book::<ByType<ProjectileSpec>>();
        d.api(ProjectilesApi::register);
    }

    /// Adds projectiles to a match. In Hit, before attacks strike, each projectile flies a step,
    /// and its hits and its end run their action's `on_hit` and `on_end`; after attacks strike
    /// and casts resolve, the tick's launches take off, and fly from the next tick. A predicting
    /// client predicts no delivery, so it flies none.
    fn install(cx: &mut InstallCx<'_>) {
        let predicting = cx.predicting();
        let schedule = &mut *cx.schedule;
        schedule.add_systems((
            Launching::take_shots.in_set(CombatSet::Fire),
            Launching::launch.in_set(CombatSet::Launch),
        ));
        if !predicting {
            schedule.add_systems(Flying::fly.in_set(DeliverySet::Fly));
        }
    }
}

impl Projectiles {
    /// The engine tag of projectile types: a filter selects their units only when it names it.
    pub(crate) const TAG: EngineTag =
        EngineTag::new(1, "projectile", TagProperties::NONE.with_named_only());

    /// Launches `fan`, the delivery of `by`, which aimed at `target` from `from`: a homing
    /// projectile at a unit for a homing type, or the fan's projectiles along lines spread evenly
    /// over its spread around the aim, which for an action aimed at a point end there at the
    /// latest; nothing for no target, a unit that is gone, or an aim of no direction.
    fn deliver(
        world: &mut World,
        by: Delivering,
        from: Position,
        delivery: Delivery,
        target: ActionTarget,
    ) {
        let Delivery {
            unit_type,
            shape: DeliveryShape::Projectile { fan, .. },
        } = delivery
        else {
            panic!("projectiles deliver a fan of projectiles");
        };
        let book = world.resource::<ActionBook>();
        let action = book.get(by.action).expect("a cast's action is in the book");
        let to_point = matches!(action.aim, Aim::Point { .. });
        let spec = *world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        if let (ActionTarget::Unit(unit), true) = (target, spec.homing) {
            let homing = Flight::homing(unit);
            Projectiles::push(world, by, unit_type, from, None, [homing]);
            return;
        }
        let Some(at) = target.point(world) else {
            return;
        };
        let offset = world.resource::<Metric>().offset(from, at);
        let Some(aim) = offset.normalized() else {
            return;
        };
        let range = Projectiles::range(&spec, action.values(by.rank).range);
        let distance = offset.length();
        let range = if to_point {
            Some(range.map_or(distance, |range| range.min(distance)))
        } else {
            range
        };
        let bounds = *world.resource::<Bounds>();
        let flights = (0..fan.count.get()).map(|at| {
            let direction = aim.rotated_y(fan.turn(at).sin_cos());
            let reach = Projectiles::reach(bounds, range, from, direction);
            Flight::line(direction, reach, target.unit())
        });
        Projectiles::push(world, by, unit_type, from, None, flights);
    }

    /// The range of a line projectile of `spec`: its type's, or else `range`, its action's at
    /// its rank; `None` for a global range.
    fn range(spec: &ProjectileSpec, range: ActionRange) -> Option<Num> {
        match (spec.range, range) {
            (Some(meters), _) | (None, ActionRange::Meters(meters)) => Some(meters),
            (None, ActionRange::Global) => None,
        }
    }

    /// The range of a line projectile of `unit_type` of `by`, as `range` gives it.
    fn line_range(world: &World, by: Delivering, unit_type: UnitType) -> Option<Num> {
        let action = world
            .resource::<ActionBook>()
            .get(by.action)
            .expect("a delivery's action is in the book");
        let spec = world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        Projectiles::range(spec, action.values(by.rank).range)
    }

    /// How far a line of `range` flies from `from` along `direction`: up to the edge of the map's
    /// bounds, which a global range flies to.
    fn reach(bounds: Bounds, range: Option<Num>, from: Position, direction: Vec3) -> Num {
        let exit = bounds.exit(from, direction);
        range.map_or(exit, |range| range.min(exit))
    }

    /// Queues `flights` of `unit_type` from `from` as one cast of `by`; with `id`, one flight,
    /// which a script launched with the id it took.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        from: Position,
        id: Option<StableId>,
        flights: impl IntoIterator<Item = Flight>,
    ) {
        let mut launches = world.resource_mut::<Launches>();
        let cast = launches.cast();
        let before = launches.len();
        launches.extend(flights.into_iter().map(|flight| Launch {
            id,
            source: by.source,
            from,
            unit_type,
            flight,
            payload: LaunchPayload::Action {
                action: by.action,
                rank: by.rank,
                start: by.start,
                cast,
            },
        }));
        debug_assert!(
            id.is_none() || launches.len() == before + 1,
            "a script's id is one flight's"
        );
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_math::{Num, Vec3};
    use campfire_sim::{EntityIndex, Position, StableId, TickRate};

    use crate::projectiles::Projectiles;
    use crate::projectiles::launches::{Launch, LaunchPayload, Launches};
    use crate::projectiles::projectile::{Flight, Payload, Projectile};
    use crate::projectiles::projectile_data::ProjectileData;
    use crate::projectiles::projectile_spec::ProjectileSpec;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::damage_kind::DamageKind;
    use crate::values::rank::Rank;

    impl Projectiles {
        /// Makes `unit_type` a projectile type of `data`, tagged `projectile`, its speed a tick at
        /// the match's rate, which the package load checked: its `hits` filter names tags of the
        /// match's.
        pub(crate) fn load_type(world: &mut World, unit_type: UnitType, data: &ProjectileData) {
            let rate = *world.resource::<TickRate>();
            let view = world.non_send::<View>().clone();
            let spec = {
                let mut types = view.types_mut();
                types.give_tag(unit_type, Projectiles::TAG.tag());
                ProjectileSpec::of(data, &types, rate)
            };
            world
                .resource_mut::<ByType<ProjectileSpec>>()
                .set(unit_type, spec);
        }

        /// Launches a projectile of the line type `unit_type` from `source` at the origin along
        /// +x, carrying 10 damage, from the next tick.
        pub(crate) fn fire_line(world: &mut World, source: StableId, unit_type: UnitType) {
            let range = world
                .resource::<ByType<ProjectileSpec>>()
                .get(unit_type)
                .unwrap()
                .range
                .unwrap();
            world.resource_mut::<Launches>().push(Launch {
                id: None,
                source,
                from: Position::ORIGIN,
                unit_type,
                flight: Flight::line(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO), range, None),
                payload: LaunchPayload::Attack {
                    action: ActionId::new(0),
                    rank: Rank::FIRST,
                    amount: Num::int(10),
                    kind: DamageKind::new(0),
                    roll: Num::ZERO,
                },
            });
        }

        /// The roll each projectile carries, by stable id.
        pub(crate) fn rolls(world: &World) -> Vec<Num> {
            let projectiles = world.resource::<EntityIndex>().iter();
            projectiles
                .filter_map(|(_, entity)| world.get::<Projectile>(entity))
                .map(|projectile| match projectile.payload() {
                    Payload::Attack { roll, .. } => roll,
                    Payload::Action { .. } => panic!("an attack's projectile"),
                })
                .collect()
        }

        /// Where each projectile is, by stable id.
        pub(crate) fn projectiles(world: &World) -> Vec<Position> {
            world
                .resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| world.get::<Projectile>(entity).is_some())
                .map(|(_, entity)| *world.get::<Position>(entity).unwrap())
                .collect()
        }
    }
}

#[cfg(test)]
mod tests;
