use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_capabilities::Order;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_mode::{OptionPicks, OptionValue, PlayerSlotPick};
use campfire_navigation::FlightData;
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_protocol::PlayerSlotPlan;
use campfire_sim::StableId;
use campfire_units::{GroundPoint, Units};
use campfire_values::{ResourceId, UnitType};

use crate::failure::CheckFailure;
use crate::match_play::{self, MatchPlay, PlayTools, START_MONEY, STEP_LIMIT};
use crate::pins::{HelicopterSteps, HelicopterTicks};

/// America's helicopters, its gatherer and its gunship.
const CHINOOK: &str = "americavehiclechinook";
const COMANCHE: &str = "americavehiclecomanche";
/// The Chinook's gather, and the docks it gathers at.
const GATHER: &str = "gather_americavehiclechinook";
const DOCK: &str = "supplydock";

/// Zero Hour's helicopters on Tournament Desert (04-capabilities/production.md, A flying gatherer
/// docks over its dock; 04-capabilities/stats.md, Airborne is a tag of height): player 1, America,
/// and player 2, GLA, each from its base start, at `Player_1_Start` and `Player_2_Start`, ordered
/// nothing but the play's orders. Player 1's first Chinook gathers at the dock nearest it and
/// brings a full load home, its other Chinooks standing, as its first Comanche climbs from the
/// ground to its height; then the Comanche flies to GLA's start, where the weapons of GLA's base
/// that hit the air shoot it down.
#[derive(Debug)]
pub(crate) struct HelicopterPlay;

impl HelicopterPlay {
    /// Plays the helicopters in a match of `packages` at `rate` ticks a second: the ticks of the
    /// Comanche's climb, of the Chinook's first load, and of the Comanche's flight to its fall,
    /// and the load's pay; a failure goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<HelicopterSteps> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let player_slot_picks: Vec<PlayerSlotPick> = [
            "0:faction=america",
            "0:start=base",
            "1:faction=gla",
            "1:start=base",
        ]
        .map(|pick| pick.parse().expect("the play's pick"))
        .into();
        let session = FixedSession::planned(packages, [PlayerSlotPlan::Player; 2])
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&player_slot_picks);
        let mut play = MatchPlay::new(session.start(), session.packages(), tools);
        let Some(money) = MatchUnits::of(play.game()).resource("money") else {
            failures.push(CheckFailure::NoMoney);
            failures.extend(play.end().err());
            return None;
        };
        let played = Self::steps(&mut play, session.packages(), money);
        let ended = play.end();
        match played.and_then(|played| ended.map(|()| played)) {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The steps in `play` of `packages`, after its start's tick, of player 1's `money`.
    fn steps(
        play: &mut MatchPlay<'_>,
        packages: &ModePackages,
        money: ResourceId,
    ) -> Result<HelicopterSteps, CheckFailure> {
        play.step();
        let chinooks = play.owned(CHINOOK, 0)?;
        let [chinook, ref others @ ..] = chinooks[..] else {
            return Err(CheckFailure::NoStartUnit {
                player_slot: 0,
                unit: CHINOOK,
            });
        };
        let [comanche, ..] = play.owned(COMANCHE, 0)?[..] else {
            return Err(CheckFailure::NoStartUnit {
                player_slot: 0,
                unit: COMANCHE,
            });
        };
        let flight = packages.content().units.get(COMANCHE);
        let flight = flight.and_then(|held| held.section::<FlightData>());
        let height = flight.ok_or(CheckFailure::NoFlight(COMANCHE))?.height();
        // The other Chinooks stand, which their supply center sent to gather, so the money that
        // comes is this one's.
        let dock = Self::nearest_dock(play, chinook)?;
        let gather = OrdersDirective::Slot {
            slot: match_play::slot(packages, CHINOOK, GATHER)?,
            target: ActionTarget::Unit(dock),
        };
        let stops = others
            .iter()
            .map(|&other| Order::one(other, OrdersDirective::Stop));
        let orders: Vec<Order> = stops.chain([Order::one(chinook, gather)]).collect();
        play.order(&orders);
        let before = play.money(money);
        let (mut climbed, mut delivered) = (None, None);
        for ticks in 1..=STEP_LIMIT {
            play.step();
            let y = MatchUnits::of(play.game()).position(comanche).y;
            climbed = climbed.or_else(|| (y >= height).then_some(ticks));
            delivered = delivered.or_else(|| (play.money(money) != before).then_some(ticks));
            if climbed.is_some() && delivered.is_some() {
                break;
            }
        }
        let climbed = climbed.ok_or(CheckFailure::NoClimb)?;
        let delivered = delivered.ok_or(CheckFailure::NoLoad)?;
        let load = play.money(money) - before;
        let gla = play.start(2)?;
        let flight = OrdersDirective::Move {
            to: GroundPoint { x: gla.x, z: gla.z },
        };
        play.order(&[Order::one(comanche, flight)]);
        let shot_down = play
            .run_until(|play| {
                play.owned(COMANCHE, 0)
                    .is_ok_and(|owned| !owned.contains(&comanche))
            })
            .ok_or(CheckFailure::NoShotDown)?;
        Ok(HelicopterSteps {
            ticks: HelicopterTicks {
                climbed,
                delivered,
                shot_down,
            },
            load,
        })
    }

    /// The supply dock nearest `unit` on the ground, the lowest stable id of a tie.
    fn nearest_dock(play: &MatchPlay<'_>, unit: StableId) -> Result<StableId, CheckFailure> {
        let units = MatchUnits::of(play.game());
        let dock: UnitType =
            Units::type_named(play.game(), DOCK).ok_or(CheckFailure::NoType(DOCK))?;
        let from = units.position(unit);
        units
            .all()
            .filter(|(_, entity)| entity.get::<UnitType>() == Some(&dock))
            .map(|(id, _)| (units.position(id).ground_offset(from).length(), id))
            .min()
            .map(|(_, id)| id)
            .ok_or(CheckFailure::NoType(DOCK))
    }
}
