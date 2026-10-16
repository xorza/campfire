use std::num::NonZeroU32;

use campfire_capabilities::Order;
use campfire_geometry::Shape;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::{Num, Vec3};
use campfire_mode::{MapData, MapPoint, MarkerData, ModeParam};
use campfire_orders::OrdersAction;
use campfire_package::ModePackages;
use campfire_sim::Position;
use campfire_units::{Body, Destination};
use campfire_values::DeclaredName;

use crate::failure::Failure;
use crate::match_play::{CENTER, DOZER, MatchPlay};

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
    /// failures it finds go to `failures`, and none when the start lacks a unit it checks.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<Failure>,
    ) -> Option<Self> {
        let map = packages.map().clone();
        let (Some(start), Some(goal)) = (
            Self::waypoint(&map, "Player_1_Start"),
            Self::waypoint(&map, "Player_2_Start"),
        ) else {
            failures.push(Failure::NoStarts);
            return None;
        };
        let mut play = MatchPlay::new(FixedSession::new(packages, rate, 1).start());
        let drive = Self::play(&mut play, &WallRect::of(&map), [start, goal], failures);
        play.end();
        match drive {
            Ok(drive) => Some(drive),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The start and the drive in `play`, from `start` to `goal`, among `walls`: a failure that
    /// ends it, and each other one in `failures`.
    fn play(
        play: &mut MatchPlay,
        walls: &[WallRect],
        [start, goal]: [Vec3; 2],
        failures: &mut Vec<Failure>,
    ) -> Result<Self, Failure> {
        play.step();
        let (buildings, dozers) = (play.owned(CENTER, 0)?, play.owned(DOZER, 0)?);
        let (&[building], &[dozer]) = (buildings.as_slice(), dozers.as_slice()) else {
            return Err(Failure::StartUnits {
                buildings: buildings.len(),
                dozers: dozers.len(),
            });
        };
        let units = MatchUnits::of(play.game());
        let center = units.pos(building);
        if [center.x, center.z] != [start.x, start.z] {
            failures.push(Failure::BuildingPlace(center));
        }
        let shape = |id| Body::shape_of(units.unit(id).get::<Body>());
        let clear = match (shape(building), shape(dozer)) {
            (Shape::Box(body), Shape::Circle(radius)) => {
                !body.closer(center, units.pos(dozer), radius.get())
            }
            _ => false,
        };
        if !clear {
            failures.push(Failure::DozerInBuilding(units.pos(dozer)));
        }
        play.order(&[Order::one(
            dozer,
            OrdersAction::Move {
                x: goal.x,
                z: goal.z,
            },
        )]);
        let mut ordered = false;
        while play.tick().get() < DRIVE_LIMIT {
            play.step();
            let tick = play.tick().get();
            let units = MatchUnits::of(play.game());
            let unit = units.unit(dozer);
            let pos = unit
                .get::<Position>()
                .expect("a unit stands somewhere")
                .get();
            if walls.iter().any(|wall| wall.holds(pos)) {
                return Err(Failure::DozerInWall { tick, pos });
            }
            let walking = unit
                .get::<Destination>()
                .is_some_and(|destination| destination.get().is_some());
            ordered |= walking;
            if ordered && !walking {
                return Ok(Self {
                    ticks: tick,
                    end: pos,
                    left: pos.ground_offset(goal).length(),
                });
            }
        }
        Err(Failure::NeverArrived)
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
    fn of(map: &MapData) -> Vec<Self> {
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
                let mut rect = Self {
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
