use std::num::NonZeroU32;

use campfire_capabilities::Order;
use campfire_declarations::ById;
use campfire_harness::MatchUnits;
use campfire_interaction::EnterSpec;
use campfire_math::{Num, Vec3};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_sim::StableId;
use campfire_units::GroundPoint;
use campfire_values::UnitType;

use crate::failure::CheckFailure;
use crate::match_play::{BaseStarts, MatchPlay, PlayTools, PlayWalk};
use crate::pins::{TransportSteps, TransportTicks};

/// America's units the play orders, and China's.
const HUMVEE: &str = "americavehiclehumvee";
const RANGER: &str = "americainfantryranger";
const DEFENDER: &str = "americainfantrymissiledefender";
const CRUSADER: &str = "americatankcrusader";
const CHINOOK: &str = "americavehiclechinook";
const CRAWLER: &str = "chinavehicletroopcrawler";

/// Where the play's units go, `[x, z]` meters from the middle of the two starts, in the open
/// ground between the map's ridges, as the combat play's, where the base's buildings stand in no
/// rider's way: the Humvee and the Chinook wait there for their riders. The Troop Crawler stands in
/// the middle, and the Humvee drives to 80 m of it, within its riders' weapons of 100 m. The
/// Chinook flies its riders 300 m east and 250 m north of the middle.
const HUMVEE_WAITS: [i64; 2] = [-100, 100];
const CHINOOK_WAITS: [i64; 2] = [-100, 200];
const CRAWLER_AT: [i64; 2] = [0, 0];
const HUMVEE_AT: [i64; 2] = [-80, 0];
const CHINOOK_TO: [i64; 2] = [300, 250];

/// The farthest a rider of the Chinook stands from its goal once out: the Chinook stops at its
/// goal, and each rider leaves at the free cell nearest it, of the pathfinder's 10 m, within its
/// body of 20.9 m and a cell's diagonal of 14.1 m past it, 35 m, rounded up.
const NEAR: Num = Num::int(40);

/// The transport play on Tournament Desert (transports.md, A container says who rides;
/// transports.md, A container may let its riders fire; transports.md, A container's fall hurts
/// its riders; transports.md, A flier lands for its riders): player 1, America, and player 2,
/// China, each from its base start, ordered nothing but the play's orders. A Chinook lands for a
/// Ranger and a Crusader, flies them across the map, and lets them out there; the Ranger and a
/// Missile Defender board a Humvee, which drives up to a Troop Crawler, and fire on it from
/// inside; America's Crusaders destroy the Crawler, whose 8 Red Guards leave it, each hurt by its
/// 10 %; and the Humvee lets its riders out.
#[derive(Debug)]
pub(crate) struct TransportPlay;

/// The units the play orders, of their players' base starts.
#[derive(Debug, Clone, Copy)]
struct TransportUnits {
    humvee: StableId,
    /// The Ranger and the Missile Defender that ride the Humvee.
    infantry: [StableId; 2],
    crusaders: [StableId; 2],
    chinook: StableId,
    crawler: StableId,
}

impl TransportPlay {
    /// Plays the transports in a match of `packages` at `rate` ticks a second: its steps, or none,
    /// with each failure in `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<TransportSteps> {
        let played = BaseStarts::AMERICA_CHINA.play(packages, rate, tools, Self::steps);
        played.map_err(|failure| failures.push(failure)).ok()
    }

    /// The play's steps in `play`, from its start's tick.
    fn steps(play: &mut MatchPlay<'_>) -> Result<TransportSteps, CheckFailure> {
        play.step();
        let units = TransportUnits::of(play)?;
        let (one, two) = (play.start(1)?, play.start(2)?);
        let at = |[x, z]: [i64; 2]| GroundPoint {
            x: (one.x + two.x) / Num::int(2) + Num::int(x),
            z: (one.z + two.z) / Num::int(2) + Num::int(z),
        };

        let waits = |unit, to| PlayWalk {
            player_slot: 0,
            unit,
            to: at(to),
        };
        play.walk(&[
            waits(units.humvee, HUMVEE_WAITS),
            waits(units.chinook, CHINOOK_WAITS),
        ])?;
        let infantry = units.infantry;
        let carried = [infantry[0], units.crusaders[1]];
        let loaded = Self::board(play, units.chinook, &carried)?;
        let to = at(CHINOOK_TO);
        play.order(&[Order::one(units.chinook, OrdersDirective::Move { to })]);
        let flown = play.step_until("the Chinook's flight", |play| {
            play.destination(units.chinook).is_none()
        })?;
        play.order(&[Order::one(units.chinook, OrdersDirective::Evacuate)]);
        let landed = play.step_until("the Chinook's riders' exit", |play| {
            carried.iter().all(|&rider| !play.riding(rider))
        })?;
        let goal = Vec3::new(to.x, Num::ZERO, to.z);
        let near = carried.iter().all(|&rider| {
            let at = MatchUnits::of(play.game()).position(rider);
            !play.dead(rider) && at.ground_offset(goal).length() <= NEAR
        });

        let boarded = Self::board(play, units.humvee, &infantry)?;
        play.walk(&[PlayWalk {
            player_slot: 1,
            unit: units.crawler,
            to: at(CRAWLER_AT),
        }])?;
        let before = infantry.map(|rider| play.times(rider).iter().map(|time| time.ready_at));
        let before = before.map(Iterator::collect::<Vec<_>>);
        let fired_by = |play: &MatchPlay<'_>| {
            infantry.iter().zip(&before).position(|(&rider, before)| {
                let times = play.times(rider).iter();
                times.zip(before).any(|(time, &was)| time.ready_at != was)
            })
        };
        let to = at(HUMVEE_AT);
        play.order(&[Order::one(units.humvee, OrdersDirective::Move { to })]);
        let fired = play.step_until("a rider's shot from the Humvee", |play| {
            fired_by(play).is_some()
        })?;
        let shooter = infantry[fired_by(play).expect("a rider fired")];
        let fired_inside = play.riding(shooter);

        let guards = play.riders(units.crawler);
        let world = play.game();
        let crawler_type = MatchUnits::of(world)
            .unit(units.crawler)
            .get::<UnitType>()
            .copied()
            .expect("a unit of a type");
        let specs = world.resource::<ById<UnitType, EnterSpec>>();
        let damage = specs
            .get(crawler_type)
            .expect("a Troop Crawler holds riders")
            .damage;
        let crusaders: Vec<Order> = units
            .crusaders
            .iter()
            .map(|&crusader| {
                let target = units.crawler;
                Order::one(crusader, OrdersDirective::Attack { target })
            })
            .collect();
        play.order(&crusaders);
        let fell = play.step_until("the Troop Crawler's fall", |play| play.dead(units.crawler))?;
        let out = guards.iter().filter(|&&guard| !play.riding(guard)).count();
        let hurt = guards.iter().all(|&guard| {
            play.life_of(guard)
                .is_some_and(|life| life.current() == life.max() - damage.of_num(life.max()))
        });

        play.order(&[Order::one(units.humvee, OrdersDirective::Evacuate)]);
        let evacuated = play.step_until("the Humvee's evacuation", |play| {
            infantry.iter().all(|&rider| !play.riding(rider))
        })?;
        Ok(TransportSteps {
            ticks: TransportTicks {
                loaded,
                flown,
                landed,
                boarded,
                fired,
                fell,
                evacuated,
            },
            fired_inside,
            out,
            hurt,
            near,
        })
    }

    /// Orders each of player 1's `riders` into `container` in the tick `play` runs next, then runs
    /// ticks until each rides; the ticks it took, the order's own among them.
    fn board(
        play: &mut MatchPlay<'_>,
        container: StableId,
        riders: &[StableId],
    ) -> Result<u64, CheckFailure> {
        let orders: Vec<Order> = riders
            .iter()
            .map(|&rider| Order::one(rider, OrdersDirective::Enter { target: container }))
            .collect();
        play.order(&orders);
        play.step_until("a boarding", |play| {
            riders.iter().all(|&rider| play.riding(rider))
        })
    }
}

impl TransportUnits {
    /// The units of `play`'s base starts the play orders: the first of each type, the first two
    /// Crusaders, and the second Ranger, as the base start encloses the first in its buildings; a
    /// failure when a start lacks one.
    fn of(play: &MatchPlay<'_>) -> Result<Self, CheckFailure> {
        let first = |unit, player_slot| -> Result<StableId, CheckFailure> {
            let owned = play.owned(unit, player_slot)?;
            owned
                .first()
                .copied()
                .ok_or(CheckFailure::NoStartUnit { player_slot, unit })
        };
        let two = |unit, player_slot| -> Result<[StableId; 2], CheckFailure> {
            match play.owned(unit, player_slot)?[..] {
                [one, two, ..] => Ok([one, two]),
                _ => Err(CheckFailure::NoStartUnit { player_slot, unit }),
            }
        };
        Ok(Self {
            humvee: first(HUMVEE, 0)?,
            infantry: [two(RANGER, 0)?[1], first(DEFENDER, 0)?],
            crusaders: two(CRUSADER, 0)?,
            chinook: first(CHINOOK, 0)?,
            crawler: first(CRAWLER, 1)?,
        })
    }
}
