// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionRange, ActionTarget, ActionsSystems, Aim, Delivery, DeliveryShape, Flights,
    ProjectileFlight,
};
use campfire_common::EnumAll;
use campfire_declarations::{ByType, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_deliveries::{Deliverers, Delivering, Launcher};
use campfire_geometry::Bounds;
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, Position, SimWorld, StableId};
use campfire_units::{Body, MapBook, UnitGrid, UnitTypes};
use campfire_values::{EngineTag, TagProperties, UnitType};

mod curve;
mod error;
mod flight_state;
mod flights;
mod flying;
mod launches;
mod launching;
mod projectile;
mod projectile_books;
mod projectile_data;
mod projectile_spec;
mod projectiles_api;
mod projectiles_effect;
mod struck_units;

pub use crate::error::{
    ProjectileDataError, ProjectileDeliveryError, ProjectileError, ProjectilesRefusal,
};
pub use crate::flight_state::FlightState;
pub use crate::flying::Flying;
pub use crate::launches::{Launch, LaunchPayload, Launches};
pub use crate::launching::Launching;
pub use crate::projectile::{Flight, Payload, Projectile};

use crate::projectile::CurveAim;
pub use crate::projectile_data::{
    CurveData, ProjectileData, ProjectileDataBuilder, ProjectileHits,
};
pub use crate::projectile_spec::ProjectileSpec;
pub use crate::projectiles_api::ProjectilesApi;
pub use crate::struck_units::{Struck, StruckUnits};

/// The `projectiles` capability: projectile units, which ranged attacks fire and actions
/// deliver. It builds on stats, whose life tells a unit from a projectile and whose move speed cap
/// a homing projectile outruns; combat's attacks fire it when a match has both.
#[derive(Debug)]
pub struct Projectiles;

/// The steps of `projectiles`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ProjectilesSystems {
    /// Projectiles fly a step, and their hits and ends run their actions' hooks.
    Fly,
    /// The ranged attacks and the actions that fire take their shots.
    TakeShots,
    /// The tick's shots launch.
    Launch,
}

impl ModuleSystems for ProjectilesSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        SetPlace::at(match self {
            Self::Fly => ActionsSystems::Travel,
            Self::TakeShots => ActionsSystems::Fire,
            Self::Launch => ActionsSystems::Launch,
        })
    }
}

impl Module for Projectiles {
    const CAPABILITY: Option<Capability> = Some(Capability::Projectiles);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Systems = ProjectilesSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ProjectileData>();
        d.engine_tags(&[Self::TAG]);
        d.component::<Projectile>();
        d.component::<FlightState>();
        d.resource::<StruckUnits>();
        d.scratch::<Launches>();
        d.callback::<Deliverers>(Capability::Projectiles, Self::deliver);
        d.require::<Projectile, FlightState>();
        d.observer(Flying::lose_teleported);
        d.observer(Flying::lose_died);
        d.extend::<UnitTypes>(Self::tag_types);
        d.fact::<Flights>(Self::flights);
        d.book::<ByType<ProjectileSpec>>(ProjectileSpec::book);
        d.rule(Self::rule);
        d.rule(Self::deliveries_rule);
        d.api(ProjectilesApi::register);
    }

    /// Adds projectiles to a match. In Hit, before attacks strike, each projectile flies a step,
    /// and its hits and its end run their action's `on_hit` and `on_end`; after attacks strike
    /// and casts resolve, the tick's launches take off, and fly from the next tick. A teleport
    /// makes each projectile that homes on its unit lose it. A predicting client predicts no
    /// delivery, so it takes no shot, launches none and flies none.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.server(ProjectilesSystems::TakeShots, Launching::take_shots);
        cx.server(ProjectilesSystems::Launch, Launching::launch);
        cx.server(
            ProjectilesSystems::Fly,
            UnitGrid::before_reading(Flying::fly),
        );
    }
}

impl Projectiles {
    /// The engine tag of projectile types: a filter selects their units only when it names it.
    pub const TAG: EngineTag =
        EngineTag::new(2, "projectile", TagProperties::NONE.with_named_only());

    /// Launches the fan of `launcher`, which aimed at `target` from `from`: a homing projectile
    /// at a unit for a homing type; a curve from its caster's center to its unit's center or to
    /// its point for a curve type; or the fan's projectiles along lines spread evenly over its
    /// spread around the aim, which for an action aimed at a point end there at the latest;
    /// nothing for no target, a unit that is gone, or an aim of no direction.
    fn deliver(world: &mut World, launcher: Launcher, from: Vec3, target: ActionTarget) {
        let Launcher {
            by,
            delivery:
                Delivery {
                    unit_type,
                    shape: DeliveryShape::Projectile { fan, .. },
                },
        } = launcher
        else {
            panic!("projectiles deliver a fan of projectiles");
        };
        let book = world.resource::<ActionBook>();
        let action = book.get(by.action).expect("a cast's action is in the book");
        let to_point = matches!(action.aim, Aim::Point { .. });
        let spec = world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        if let (ActionTarget::Unit(unit), true) = (target, spec.homing) {
            let homing = Flight::homing(unit);
            Self::push(world, by, unit_type, from, None, [homing]);
            return;
        }
        if spec.flight() == ProjectileFlight::Curve {
            let map = world.resource::<MapBook>();
            let to = match target {
                ActionTarget::Unit(unit) => Self::center(world, unit),
                ActionTarget::Point(at) => Some(map.point(at)),
                ActionTarget::None => None,
            };
            let Some(to) = to else {
                return;
            };
            let body = world
                .entity_of(by.source)
                .and_then(|source| world.get::<Body>(source));
            let aim = CurveAim {
                from: Body::center(body, from, map),
                to,
                aimed: target.unit(),
            };
            let launch = map.position(aim.from);
            let curve = Flight::curve(aim, spec, map);
            Self::push(world, by, unit_type, launch, None, [curve]);
            return;
        }
        let Some(at) = target.position(world) else {
            return;
        };
        let offset = world.resource::<MapBook>().metric().offset(from, at);
        let Some(aim) = offset.normalized() else {
            return;
        };
        let range = Self::range(spec, action.values(by.rank).range);
        let distance = offset.length();
        let range = if to_point {
            Some(range.map_or(distance, |range| range.min(distance)))
        } else {
            range
        };
        let bounds = world.resource::<MapBook>().bounds;
        let flights = (0..fan.count.get()).map(|at| {
            let direction = aim.rotated_y(fan.turn(at).sin_cos());
            let reach = Self::reach(bounds, range, from, direction);
            Flight::line(from, direction, reach, target.unit())
        });
        Self::push(world, by, unit_type, from, None, flights);
    }

    /// The center of the unit `id` in `world`, as `Body::center` gives it; none for a unit that is
    /// gone.
    fn center(world: &World, id: StableId) -> Option<Vec3> {
        let entity = world.entity_of(id)?;
        let position = world.get::<Position>(entity)?.get();
        let body = world.get::<Body>(entity);
        Some(Body::center(body, position, world.resource::<MapBook>()))
    }

    /// The range of a line projectile of `spec`: its type's, or else `range`, its action's at
    /// its rank; `None` for a global range.
    const fn range(spec: &ProjectileSpec, range: ActionRange) -> Option<Num> {
        match (spec.range, range) {
            (Some(meters), _) => Some(meters),
            (None, ActionRange::Meters(meters)) => Some(meters.get()),
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
        Self::range(spec, action.values(by.rank).range)
    }

    /// How far a line of `range` flies from `from` along `direction`: up to the edge of the map's
    /// bounds, which a global range flies to.
    fn reach(bounds: Bounds, range: Option<Num>, from: Vec3, direction: Vec3) -> Num {
        let exit = bounds.exit(from, direction);
        range.map_or(exit, |range| range.min(exit))
    }

    /// Queues `flights` of `unit_type` from `from` as one cast of `by`; with `id`, one flight,
    /// which a script launched with the id it took.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        from: Vec3,
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

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_actions::ShotDamage;
    use campfire_declarations::ByType;
    use campfire_declarations::internals::BookLoad;
    use campfire_math::{Num, Vec3};
    use campfire_sim::{EntityIndex, MatchRate, Position, StableId};
    use campfire_units::{MapBook, UnitTypeBook, Units};
    use campfire_values::{ActionId, DamageKind, Rank, UnitType};

    use crate::Projectiles;
    use crate::launches::{Launch, LaunchPayload, Launches};
    use crate::projectile::{Flight, Payload, Projectile};
    use crate::projectile_data::ProjectileData;
    use crate::projectile_spec::ProjectileSpec;

    impl Projectiles {
        /// Makes `unit_type` a projectile type of `data`, tagged `projectile`, its speed a tick at
        /// the match's rate: its `hits` filter names tags of the match's.
        pub fn load_type(world: &mut World, unit_type: UnitType, data: &ProjectileData) {
            let rate = *world.resource::<MatchRate>();
            Units::change_types(world, |_, types| types.give_tag(unit_type, Self::TAG.tag()));
            let tags = world.resource::<UnitTypeBook>().tags();
            let spec = ProjectileSpec::of(data, tags, rate)
                .expect("a test's projectile hits tags its match has");
            ByType::<ProjectileSpec>::amend(world, |_, book| book.set(unit_type, spec));
        }

        /// Launches a projectile of the line type `unit_type` from `source` at the origin along
        /// the unit vector `direction`, up to the map's bounds as a cast flies, carrying 10
        /// damage, from the next tick.
        pub fn fire_line(
            world: &mut World,
            source: StableId,
            unit_type: UnitType,
            direction: Vec3,
        ) {
            let range = world
                .resource::<ByType<ProjectileSpec>>()
                .get(unit_type)
                .unwrap()
                .range;
            let bounds = world.resource::<MapBook>().bounds;
            let reach = Self::reach(bounds, range, Vec3::ZERO, direction);
            world.resource_mut::<Launches>().push(Launch {
                id: None,
                source,
                from: Vec3::ZERO,
                unit_type,
                flight: Flight::line(Vec3::ZERO, direction, reach, None),
                payload: LaunchPayload::Attack(ShotDamage {
                    action: ActionId::new(0),
                    rank: Rank::FIRST,
                    amount: Num::int(10),
                    kind: DamageKind::new(0),
                    roll: Num::ZERO,
                }),
            });
        }

        /// The roll each projectile carries, by stable id.
        pub fn rolls(world: &World) -> Vec<Num> {
            let projectiles = world.resource::<EntityIndex>().iter();
            projectiles
                .filter_map(|(_, entity)| world.get::<Projectile>(entity))
                .map(|projectile| match projectile.payload() {
                    Payload::Attack(damage) => damage.roll,
                    Payload::Action { .. } => panic!("an attack's projectile"),
                })
                .collect()
        }

        /// Where each projectile is, by stable id.
        pub fn projectiles(world: &World) -> Vec<Vec3> {
            world
                .resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| world.get::<Projectile>(entity).is_some())
                .map(|(_, entity)| world.get::<Position>(entity).unwrap().get())
                .collect()
        }
    }
}
