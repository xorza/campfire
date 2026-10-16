#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionRange, ActionTarget, Aim, Delivery, DeliveryShape};
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_deliveries::{Deliverers, Delivering, DeliveryKind, Launcher};
use campfire_geometry::Bounds;
use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, StableId};
use campfire_units::MapBook;
use campfire_values::{EngineTag, TagProperties, UnitType};

mod error;
mod flight_state;
mod flights;
mod flying;
mod launches;
mod launching;
mod projectile;
mod projectile_data;
mod projectile_spec;
mod projectiles_api;
mod projectiles_effect;
mod struck_units;

pub use crate::error::ProjectilesRefusal;
pub use crate::flight_state::FlightState;
pub use crate::flying::Flying;
pub use crate::launches::{Launch, LaunchPayload, Launches};
pub use crate::launching::Launching;
pub use crate::projectile::{Flight, Payload, Projectile};
pub use crate::projectile_data::{ProjectileData, ProjectileHits};
pub use crate::projectile_spec::ProjectileSpec;
pub use crate::projectiles_api::ProjectilesApi;
pub use crate::struck_units::{Struck, StruckUnits};

/// The `projectiles` capability: projectile units, which ranged attacks fire and actions
/// deliver. It builds on combat, which a match installs too.
#[derive(Debug)]
pub struct Projectiles;

campfire_common::enum_all! {
    /// The steps of `projectiles`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum ProjectilesSet {
        /// Projectiles fly a step, and their hits and ends run their actions' hooks.
        Fly,
        /// The ranged attacks and the actions that fire take their shots.
        TakeShots,
        /// The tick's shots launch.
        Launch,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for ProjectilesSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Projectiles);
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            Self::Fly => Phase::Fly,
            Self::TakeShots => Phase::Fire,
            Self::Launch => Phase::Launch,
        })
    }
}

impl Module for Projectiles {
    const CAPABILITY: Option<Capability> = Some(Capability::Projectiles);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.engine_tags(&[Self::TAG]);
        d.sets::<ProjectilesSet>();
        d.component::<Projectile>();
        d.component::<FlightState>();
        d.resource::<StruckUnits>();
        d.scratch::<Launches>();
        d.callback::<Deliverers>(DeliveryKind::Projectile, Self::deliver);
        d.require::<Projectile, FlightState>();
        d.observer(Flying::lose_teleported);
        d.book::<ByType<ProjectileSpec>>();
        d.api(ProjectilesApi::register);
    }

    /// Adds projectiles to a match. In Hit, before attacks strike, each projectile flies a step,
    /// and its hits and its end run their action's `on_hit` and `on_end`; after attacks strike
    /// and casts resolve, the tick's launches take off, and fly from the next tick. A teleport
    /// makes each projectile that homes on its unit lose it. A predicting client predicts no
    /// delivery, so it flies none.
    fn install(cx: &mut InstallCx<'_>) {
        cx.systems((
            Launching::take_shots.in_set(ProjectilesSet::TakeShots),
            Launching::launch.in_set(ProjectilesSet::Launch),
        ));
        cx.server(Flying::fly.in_set(ProjectilesSet::Fly));
    }
}

impl Projectiles {
    /// The engine tag of projectile types: a filter selects their units only when it names it.
    pub const TAG: EngineTag =
        EngineTag::new(2, "projectile", TagProperties::NONE.with_named_only());

    /// Launches the fan of `launcher`, which aimed at `target` from `from`: a homing projectile
    /// at a unit for a homing type, or the fan's projectiles along lines spread evenly over its
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
        let spec = *world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        if let (ActionTarget::Unit(unit), true) = (target, spec.homing) {
            let homing = Flight::homing(unit);
            Self::push(world, by, unit_type, from, None, [homing]);
            return;
        }
        let Some(at) = target.pos(world) else {
            return;
        };
        let offset = world.resource::<MapBook>().metric.offset(from, at);
        let Some(aim) = offset.normalized() else {
            return;
        };
        let range = Self::range(&spec, action.values(by.rank).range);
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
            Flight::line(direction, reach, target.unit())
        });
        Self::push(world, by, unit_type, from, None, flights);
    }

    /// The range of a line projectile of `spec`: its type's, or else `range`, its action's at
    /// its rank; `None` for a global range.
    const fn range(spec: &ProjectileSpec, range: ActionRange) -> Option<Num> {
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
    use campfire_declare::ByType;
    use campfire_math::{Num, Vec3};
    use campfire_sim::{EntityIndex, Position, StableId, TickRate};
    use campfire_units::UnitTypeBook;
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
            let rate = *world.resource::<TickRate>();
            let mut types = world.resource_mut::<UnitTypeBook>();
            types.give_tag(unit_type, Self::TAG.tag());
            let spec = ProjectileSpec::of(data, world.resource::<UnitTypeBook>(), rate)
                .expect("a test's projectile hits tags its match has");
            world
                .resource_mut::<ByType<ProjectileSpec>>()
                .set(unit_type, spec);
        }

        /// Launches a projectile of the line type `unit_type` from `source` at the origin along
        /// +x, carrying 10 damage, from the next tick.
        pub fn fire_line(world: &mut World, source: StableId, unit_type: UnitType) {
            let range = world
                .resource::<ByType<ProjectileSpec>>()
                .get(unit_type)
                .unwrap()
                .range
                .unwrap();
            world.resource_mut::<Launches>().push(Launch {
                id: None,
                source,
                from: Vec3::ZERO,
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
        pub fn rolls(world: &World) -> Vec<Num> {
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
