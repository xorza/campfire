use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_bots::{BotNames, BotScript, Location, OrderScript, ScriptedOrder, ScriptedUnits};
use campfire_capabilities::Directive;
use campfire_common::DeclaredName;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::{OptionPicks, OptionValue};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, SeatPlan};
use campfire_units::{Destination, MapPoint};

use crate::failure::CheckFailure;
use crate::match_play::{
    self, BuildingDozer, DOZER, MatchPlay, PlannedBuilding, STEP_LIMIT, america,
};
use crate::pins::DefenseTicks;

/// The money player 1 starts with, as the skirmish play's host picks it.
const START_MONEY: &str = "start_money=50000";

/// The barracks, which the dozer places where the skirmish play places its own, 216 m along −z
/// from `Player_1_Start`, clear of the cliffs, at America's `PlacementViewAngle` of −135°.
const BARRACKS: PlannedBuilding = PlannedBuilding {
    unit: "americabarracks",
    action: "build_americabarracks",
    at: [1005, -800, -135],
};

/// The barracks' ranger, which defends from where it stands once it left the barracks.
const RANGER: &str = "americainfantryranger";

/// How far south of the ranger the bot's dozer drives, in meters: within the 100 m its rifle
/// reaches from its edge to the dozer's.
const SHORT_OF_THE_RANGER: Num = Num::int(60);

/// An idle unit's defense on Tournament Desert (04-capabilities/control.md, An idle unit scans
/// for a target, as its type declares): player 1 builds a barracks, which trains a ranger, and
/// the bot drives its dozer from its start to a point within the ranger's reach, where the ranger,
/// ordered nothing, scans, fires, and kills it.
#[derive(Debug)]
pub(crate) struct DefensePlay;

impl DefensePlay {
    /// Plays the defense in a match of `packages` at `rate` ticks a second: the barracks' ticks
    /// and the ranger's, and the ticks from the dozer's order to its first damage and to its
    /// death; a failure for a step that does not end, a unit or an action the play lacks, or an
    /// option the mode does not take, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<DefenseTicks> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let plan = [SeatPlan::Player, SeatPlan::Bot];
        let session = FixedSession::planned(packages, InputRules::ROOMY, plan)
            .rate(rate)
            .picked(picks.places())
            .seat_picked(&america(2));
        let mut play = MatchPlay::new(session.start());
        let played = Self::steps(&mut play, session.packages());
        play.end();
        match played {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The steps in `play` of `packages`, after its start's tick.
    fn steps(play: &mut MatchPlay, packages: &ModePackages) -> Result<DefenseTicks, CheckFailure> {
        play.step();
        let [dozer] = play.owned(DOZER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let builder = BuildingDozer {
            unit: DOZER,
            id: dozer,
        };
        let barracks_ticks = play.build(packages, builder, BARRACKS)?;
        let [barracks] = play.owned(BARRACKS.unit, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let train = OrdersDirective::Slot {
            slot: match_play::slot(packages, BARRACKS.unit, "train_americainfantryranger")?,
            target: ActionTarget::None,
        };
        let ranger_ticks =
            play.until(
                barracks,
                train,
                |play| Ok(play.owned(RANGER, 0)?.len() == 1),
            )?;
        let [ranger] = play.owned(RANGER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        // The ranger walks from the barracks' door to its exit point; the dozer drives to where it
        // then stands.
        let walks = |play: &MatchPlay| {
            let unit = MatchUnits::of(play.game()).unit(ranger);
            let destination = unit.get::<Destination>();
            destination.is_some_and(|destination| destination.get().is_some())
        };
        let mut walked = 0;
        while walks(play) {
            walked += 1;
            if walked > STEP_LIMIT {
                return Err(CheckFailure::RangerWalk);
            }
            play.step();
        }
        let at = MatchUnits::of(play.game()).position(ranger);
        let to = MapPoint::Ground([at.x, at.z - SHORT_OF_THE_RANGER]);
        let drive = ScriptedOrder {
            tick: play.tick(),
            units: ScriptedUnits::Typed(DeclaredName::new(DOZER).expect("the dozer's name")),
            directive: Directive::Orders(OrdersDirective::Move {
                to: Location::At(to),
            }),
        };
        let script = OrderScript::of_orders([drive]).map_err(CheckFailure::BotOrders)?;
        let books = packages.name_books();
        let bot =
            BotScript::new(&script, &BotNames::new(&books)).map_err(CheckFailure::BotNames)?;
        play.drive_bot(1, bot)?;
        let [enemy] = play.owned(DOZER, 1)?[..] else {
            return Err(CheckFailure::FightStart);
        };
        let full = play.life(enemy);
        let mut hit = None;
        for ticks in 1..=STEP_LIMIT {
            play.step();
            let life = play.life(enemy);
            if hit.is_none() && life < full {
                hit = Some(ticks);
            }
            if let Some(hit) = hit
                && life.is_none_or(|life| life == Num::ZERO)
            {
                return Ok(DefenseTicks {
                    built: [barracks_ticks, ranger_ticks],
                    hit,
                    death: ticks,
                });
            }
        }
        Err(CheckFailure::Undefended)
    }
}
