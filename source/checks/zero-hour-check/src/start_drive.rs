use std::cmp::Ordering;
use std::num::NonZeroU32;

use campfire_capabilities::Order;
use campfire_common::Tick;
use campfire_geometry::Shape;
use campfire_harness::{FixedMatch, FixedSession, MatchUnits};
use campfire_math::{Num, Vec3};
use campfire_mode::{MapData, MapPoint, MarkerData, ModeParam};
use campfire_orders::OrdersAction;
use campfire_package::ModePackages;
use campfire_runner::SessionWorld;
use campfire_sim::{Position, StableId};
use campfire_units::{Body, Destination, Units};
use campfire_values::{DeclaredName, UnitType};

use crate::failure::Failure;

/// The unit type of America's command center, the building a skirmish starts a player with.
const BUILDING: &str = "americacommandcenter";

/// The unit type of America's dozer, the unit a skirmish starts a player with.
const DOZER: &str = "americavehicledozer";

/// The most ticks the check waits for the dozer's walk to end, past which it never arrives.
const DRIVE_LIMIT: u64 = 30 * 60 * 10;

/// The skirmish's start of Tournament Desert and the dozer's drive (design 17, D6): player 1's
/// command center at `Player_1_Start` and its dozer clear of the center's box, both its own; then
/// the dozer, ordered to `Player_2_Start`, which stands in no wall's cell in any tick, and ends
/// its walk `ticks` later at `end`, `left` from the point it was ordered to.
#[derive(Debug)]
pub(crate) struct StartDrive {
    pub(crate) ticks: u64,
    pub(crate) end: Vec3,
    pub(crate) left: Num,
}

/// A wall of the map as the rectangle it covers, its least and its greatest corner, `[x, z]`.
#[derive(Debug, Clone, Copy)]
struct WallRect {
    min: [Num; 2],
    max: [Num; 2],
}

impl StartDrive {
    /// Plays the start and the drive in a match of `packages` at `rate` ticks a second; the
    /// failures it finds go to `failures`, and none when the start lacks a unit it checks. The
    /// match takes the log's events while it lives, as its harness checks them, so it ends
    /// before the check logs again.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<Failure>,
    ) -> Option<StartDrive> {
        let map = packages.map().clone();
        let (Some(start), Some(goal)) = (
            StartDrive::waypoint(&map, "Player_1_Start"),
            StartDrive::waypoint(&map, "Player_2_Start"),
        ) else {
            failures.push(Failure::NoStarts);
            return None;
        };
        let mut fixed = FixedSession::new(packages, rate, 1).start();
        let drive = StartDrive::play(&mut fixed, &WallRect::of(&map), [start, goal], failures);
        fixed.end();
        drive
    }

    /// The start and the drive in `fixed`, from `start` to `goal`, among `walls`.
    fn play(
        fixed: &mut FixedMatch,
        walls: &[WallRect],
        [start, goal]: [Vec3; 2],
        failures: &mut Vec<Failure>,
    ) -> Option<StartDrive> {
        let mut tick = 0;
        fixed.game_mut().run_tick_alone();
        tick += 1;
        let world = fixed.game();
        let units = MatchUnits::of(world);
        let typed = |name: &str| Units::type_named(world, name).expect("a start type of the mode");
        let owned = |unit_type: UnitType| -> Vec<StableId> {
            units
                .all()
                .filter(|(_, unit)| {
                    unit.get::<UnitType>() == Some(&unit_type) && MatchUnits::owned_by(unit, 0)
                })
                .map(|(id, _)| id)
                .collect()
        };
        let (buildings, dozers) = (owned(typed(BUILDING)), owned(typed(DOZER)));
        let (&[building], &[dozer]) = (buildings.as_slice(), dozers.as_slice()) else {
            failures.push(Failure::StartUnits {
                buildings: buildings.len(),
                dozers: dozers.len(),
            });
            return None;
        };
        let center = units.pos(building);
        if [center.x, center.z] != [start.x, start.z] {
            failures.push(Failure::BuildingPlace(center));
        }
        let shape = |id| Body::shape_of(units.unit(id).get::<Body>());
        let clear = match (shape(building), shape(dozer)) {
            (Shape::Box(body), Shape::Circle(radius)) => {
                body.nearest(center, units.pos(dozer), radius.get()) != Ordering::Less
            }
            _ => false,
        };
        if !clear {
            failures.push(Failure::DozerInBuilding(units.pos(dozer)));
        }
        let order = Order::one(
            dozer,
            OrdersAction::Move {
                x: goal.x,
                z: goal.z,
            },
        );
        fixed.send(0, Tick::new(tick), &Order::payload(&[order]));
        let mut ordered = false;
        while tick < DRIVE_LIMIT {
            fixed.game_mut().run_tick_alone();
            tick += 1;
            let units = MatchUnits::of(fixed.game());
            let unit = units.unit(dozer);
            let pos = unit
                .get::<Position>()
                .expect("a unit stands somewhere")
                .get();
            if walls.iter().any(|wall| wall.holds(pos)) {
                failures.push(Failure::DozerInWall { tick, pos });
                return None;
            }
            let walking = unit
                .get::<Destination>()
                .is_some_and(|destination| destination.get().is_some());
            ordered |= walking;
            if ordered && !walking {
                return Some(StartDrive {
                    ticks: tick,
                    end: pos,
                    left: pos.ground_offset(goal).length(),
                });
            }
        }
        failures.push(Failure::NeverArrived);
        None
    }

    /// The point of the waypoint `name` of `map`, of its start markers.
    fn waypoint(map: &MapData, name: &str) -> Option<Vec3> {
        let start = DeclaredName::new("start").expect("a declared name");
        let named = |marker: &&MarkerData| {
            marker.params.iter().any(|(key, param)| {
                key.as_str() == "name" && *param == ModeParam::Text(name.to_owned())
            })
        };
        let marker = map
            .markers
            .iter()
            .filter(|marker| marker.tags.contains(&start))
            .find(named)?;
        match marker.pos {
            Some(MapPoint::Ground([x, z])) => Some(Vec3::new(x, Num::ZERO, z)),
            _ => None,
        }
    }
}

impl WallRect {
    /// The walls of `map`'s navigation, each the rectangle of its points.
    fn of(map: &MapData) -> Vec<WallRect> {
        let walls = map
            .navigation
            .iter()
            .flat_map(|navigation| &navigation.walls);
        walls
            .map(|wall| {
                let ground = wall.points.iter().filter_map(|point| match point {
                    MapPoint::Ground(at) => Some(*at),
                    MapPoint::Space(_) => None,
                });
                let mut rect = WallRect {
                    min: [Num::MAX; 2],
                    max: [Num::MIN; 2],
                };
                for at in ground {
                    let axes = rect.min.iter_mut().zip(&mut rect.max).zip(at);
                    for ((min, max), value) in axes {
                        *min = (*min).min(value);
                        *max = (*max).max(value);
                    }
                }
                rect
            })
            .collect()
    }

    /// Whether `pos` lies inside it on the ground plane, its edges, which a free cell shares,
    /// left out.
    fn holds(self, pos: Vec3) -> bool {
        [pos.x, pos.z]
            .into_iter()
            .enumerate()
            .all(|(axis, at)| self.min[axis] < at && at < self.max[axis])
    }
}
