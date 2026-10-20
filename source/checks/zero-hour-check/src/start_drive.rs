use std::num::NonZeroU32;

use campfire_capabilities::Order;
use campfire_geometry::{Polygon, Shape};
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_mode::internals::MarkerOf;
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_sim::Position;
use campfire_units::{Body, Destination, GroundPoint, WallBook};

use crate::failure::Failure;
use crate::match_play::{CENTER, DOZER, MatchPlay};
use crate::pins::HZ;

/// The most ticks the check waits for the dozer's walk to end, past which it never arrives.
const DRIVE_LIMIT: u64 = HZ * 60 * 10;

/// The skirmish's start of Tournament Desert and the dozer's drive (09-zero-hour.md, A skirmish
/// starts with the faction's start objects): player 1's command center at `Player_1_Start` and its
/// dozer clear of the center's box, both its own; then the dozer, ordered to `Player_2_Start`,
/// which stands inside no wall in any tick, and ends its walk `ticks` later at `end`, `left` from
/// the point it was ordered to. The starts and the walls are the match's own, as its map holds
/// them.
#[derive(Debug)]
pub(crate) struct StartDrive {
    pub(crate) ticks: u64,
    pub(crate) end: Vec3,
    pub(crate) left: Num,
}

impl StartDrive {
    /// Plays the start and the drive in a match of `packages` at `rate` ticks a second; the
    /// failures it finds go to `failures`, and none when the start lacks a unit it checks.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<Failure>,
    ) -> Option<Self> {
        let mut play = MatchPlay::new(FixedSession::new(packages, rate, 1).start());
        let start = |name| {
            let of = MarkerOf {
                tag: "start",
                param: "name",
                value: name,
            };
            Mode::marker_point(play.game(), of)
        };
        let (Some(start), Some(goal)) = (start("Player_1_Start"), start("Player_2_Start")) else {
            failures.push(Failure::NoStarts);
            play.end();
            return None;
        };
        let drive = Self::play(&mut play, [start, goal], failures);
        play.end();
        match drive {
            Ok(drive) => Some(drive),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The start and the drive in `play`, from `start` to `goal`, among the walls of the match on
    /// the dozer's layer: a failure that ends it, and each other one in `failures`.
    fn play(
        play: &mut MatchPlay,
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
        let center = units.position(building);
        if [center.x, center.z] != [start.x, start.z] {
            failures.push(Failure::BuildingPlace(center));
        }
        let shape = |id| Body::shape_of(units.unit(id).get::<Body>());
        let clear = match (shape(building), shape(dozer)) {
            (Shape::Box(body), Shape::Circle(radius)) => {
                !body.closer(center, units.position(dozer), radius.get())
            }
            _ => false,
        };
        if !clear {
            failures.push(Failure::DozerInBuilding(units.position(dozer)));
        }
        let layer = Body::layer_of(units.unit(dozer).get::<Body>());
        let walls: Vec<Polygon> = (play.game().resource::<WallBook>().walls().iter())
            .filter(|wall| wall.layer == layer)
            .map(|wall| wall.area.clone())
            .collect();
        play.order(&[Order::one(
            dozer,
            OrdersDirective::Move {
                to: GroundPoint {
                    x: goal.x,
                    z: goal.z,
                },
            },
        )]);
        let mut ordered = false;
        while play.tick().get() < DRIVE_LIMIT {
            play.step();
            let tick = play.tick().get();
            let units = MatchUnits::of(play.game());
            let unit = units.unit(dozer);
            let position = unit
                .get::<Position>()
                .expect("a unit stands somewhere")
                .get();
            if walls
                .iter()
                .any(|wall| wall.holds_inside([position.x, position.z]))
            {
                return Err(Failure::DozerInWall { tick, position });
            }
            let walking = unit
                .get::<Destination>()
                .is_some_and(|destination| destination.get().is_some());
            ordered |= walking;
            if ordered && !walking {
                return Ok(Self {
                    ticks: tick,
                    end: position,
                    left: position.ground_offset(goal).length(),
                });
            }
        }
        Err(Failure::NeverArrived)
    }
}
