use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_capabilities::{Directive, Order};
use campfire_common::PlayerSlot;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_mode::{OptionPicks, OptionValue, PlayerSlotPick};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_players::{OrderUnits, PlayersWorld};
use campfire_production::TrainQueue;
use campfire_progression::{PerkBook, PlayerPerks, PlayerPoints, ProgressionDirective};
use campfire_protocol::PlayerSlotPlan;

use crate::failure::CheckFailure;
use crate::match_play::{self, MatchPlay, PlayTools, START_MONEY, STEP_LIMIT};
use crate::pins::ScienceSteps;

/// America's war factory, its Paladin, the Paladin's train, and the science that lets it.
const FACTORY: &str = "americawarfactory";
const PALADIN: &str = "americatankpaladin";
const TRAIN: &str = "train_americatankpaladin";
const SCIENCE: &str = "science_paladintank";

/// The ticks the play runs after a train its player may not start: past the tick its order
/// applies in, as a queue would hold a started train.
const REFUSED_TICKS: u64 = 3;

/// The science play on Tournament Desert (04-capabilities/progression.md, A science is a perk of a
/// player; 04-capabilities/progression.md, A perk's modifier is a player modifier): America starts
/// with its base, whose war factory may not train a Paladin, which needs `SCIENCE_PaladinTank`; the
/// player learns that science for its rank 1 point, by an order of no units, and the war factory
/// then trains one.
#[derive(Debug)]
pub(crate) struct SciencePlay;

impl SciencePlay {
    /// Plays the science in a match of `packages` at `rate` ticks a second: how many Paladins the
    /// war factory queued before the science, the points left once it is learned, and the ticks
    /// the Paladin took; a failure for a step that does not end, a unit, an action or a perk the
    /// play lacks, or an option or a pick the mode does not take, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<ScienceSteps> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let america: Vec<PlayerSlotPick> = ["0:faction=america", "0:start=base"]
            .map(|pick| pick.parse().expect("the play's pick"))
            .into();
        let session = FixedSession::planned(packages, [PlayerSlotPlan::Player])
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&america);
        let mut play = MatchPlay::new(session.start(), session.packages(), tools);
        let played = Self::steps(&mut play, session.packages());
        let ended = play.end();
        match played.and_then(|played| ended.map(|()| played)) {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The play's steps in `play`, of `packages`, from its start's tick.
    fn steps(
        play: &mut MatchPlay<'_>,
        packages: &ModePackages,
    ) -> Result<ScienceSteps, CheckFailure> {
        play.step();
        let [factory] = play.owned(FACTORY, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let train = OrdersDirective::Slot {
            slot: match_play::slot(packages, FACTORY, TRAIN)?,
            target: ActionTarget::None,
        };
        play.order(&[Order::one(factory, train)]);
        for _ in 0..REFUSED_TICKS {
            play.step();
        }
        let queued = MatchUnits::of(play.game())
            .unit(factory)
            .get::<TrainQueue>()
            .map_or(0, |queue| queue.entries().len());
        let queued_before = queued + play.owned(PALADIN, 0)?.len();
        let perk = play
            .game()
            .get_resource::<PerkBook>()
            .and_then(|book| book.names().named(SCIENCE))
            .ok_or(CheckFailure::NoPerk(SCIENCE))?;
        let learn = Order {
            units: OrderUnits::NONE,
            directive: Directive::from(ProgressionDirective::LearnPerk { perk }),
        };
        play.order(&[learn]);
        play.step();
        let player = play.game().player_entity(PlayerSlot::new(0));
        let player = player.ok_or(CheckFailure::ProductionStart)?;
        let held = play.game().get::<PlayerPerks>(player);
        if !held.is_some_and(|held| held.holds(perk)) {
            return Err(CheckFailure::NoPerk(SCIENCE));
        }
        let points = play.game().get::<PlayerPoints>(player);
        let points = points.map_or(0, |points| points.get());
        play.order(&[Order::one(factory, train)]);
        for ticks in 1..=STEP_LIMIT {
            play.step();
            if !play.owned(PALADIN, 0)?.is_empty() {
                return Ok(ScienceSteps {
                    queued_before,
                    points,
                    paladin: ticks,
                });
            }
        }
        Err(CheckFailure::ProductionStep(train))
    }
}
