use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_capabilities::Order;
use campfire_common::PlayerSlot;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::MatchEnd;
use campfire_orders::OrdersAction;
use campfire_package::ModePackages;
use campfire_players::{OrderUnits, PlayersWorld};
use campfire_production::{BuildTarget, Site, TrainQueue};
use campfire_protocol::{InputRules, SlotPlan};

use campfire_sim::StableId;
use campfire_stats::Life;
use campfire_units::UnitsWorld;
use campfire_values::{DeclaredName, Names, ResourceId};

use crate::failure::Failure;
use crate::match_play::{CENTER, DOZER, MatchPlay};
use crate::pins::{FightSteps, ProductionSteps};

/// The unit types the play makes beside the start's.
const BARRACKS: &str = "americabarracks";
const RANGER: &str = "americainfantryranger";

/// The most ticks the play waits for a step, past which it never ends.
const STEP_LIMIT: u64 = 30 * 60 * 5;

/// How many rangers the barracks trains and sends to fight, the first of the production steps
/// among them.
const RANGERS: usize = 20;

/// The most trains a barracks queues, its `MaxQueueEntries`, which the play keeps it below.
const QUEUE: usize = 9;

/// Where the dozer places the barracks: 216 m along −z from `Player_1_Start`, clear of the
/// command center's box and of the cliffs, at America's `PlacementViewAngle` of −135°.
const BARRACKS_AT: [i64; 3] = [1005, -800, -135];

/// The skirmish on Tournament Desert against an idle bot (designs 18 and 19): player 1's command
/// center trains a dozer, its start dozer builds a barracks, and the barracks trains a ranger;
/// then the barracks trains `RANGERS` in all, and they attack the bot's command center until the
/// match ends.
#[derive(Debug)]
pub(crate) struct SkirmishPlay;

/// What the play measures: its production steps, and its fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkirmishSteps {
    pub(crate) production: ProductionSteps,
    pub(crate) fight: FightSteps,
}

/// What the play reads of its match: player 1's money, and the slots of its types' actions.
#[derive(Debug)]
struct Play<'a> {
    packages: &'a ModePackages,
    money: ResourceId,
}

impl SkirmishPlay {
    /// Plays the skirmish in a match of `packages` at `rate` ticks a second, player 1 against an
    /// idle bot: each production step's ticks and the money left after each, and the fight's; a
    /// failure for a step that does not end, or a unit the play lacks, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<Failure>,
    ) -> Option<SkirmishSteps> {
        let names: Names<ResourceId> = packages
            .data()
            .resources
            .iter()
            .map(DeclaredName::as_str)
            .collect();
        let Some(money) = names.named("money") else {
            failures.push(Failure::NoMoney);
            return None;
        };
        let plan = [SlotPlan::Player, SlotPlan::Bot];
        let session = FixedSession::planned(packages, rate, InputRules::ROOMY, plan);
        let play = Play {
            packages: session.packages(),
            money,
        };
        let mut playing = MatchPlay::new(session.start());
        let played = play.steps(&mut playing);
        playing.end();
        match played {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }
}

impl Play<'_> {
    /// The production steps in `play`, after its start's tick, then the fight.
    fn steps(&self, play: &mut MatchPlay) -> Result<SkirmishSteps, Failure> {
        play.step();
        let [center] = play.owned(CENTER, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        let [dozer] = play.owned(DOZER, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        // The center trains a dozer: its second, once its time ends.
        let train = OrdersAction::Slot {
            slot: self.slot(CENTER, "train_americavehicledozer")?,
            target: ActionTarget::None,
        };
        let dozer_ticks = self.until(play, center, train, |play| {
            Ok(play.owned(DOZER, 0)?.len() == 2)
        })?;
        let after_dozer = self.money(play);
        // The start dozer places the barracks as the order applies, and builds it whole.
        let [x, z, angle] = BARRACKS_AT.map(Num::int);
        let build = OrdersAction::Build {
            slot: self.slot(DOZER, "build_americabarracks")?,
            target: BuildTarget::Point { x, z, angle },
        };
        let barracks_ticks = self.until(play, dozer, build, |play| {
            let built = play.owned(BARRACKS, 0)?;
            let units = MatchUnits::of(play.game());
            Ok(built.len() == 1 && !units.unit(built[0]).contains::<Site>())
        })?;
        let after_barracks = self.money(play);
        let [barracks] = play.owned(BARRACKS, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        // The barracks trains a ranger, which needs the barracks whole.
        let ranger = OrdersAction::Slot {
            slot: self.slot(BARRACKS, "train_americainfantryranger")?,
            target: ActionTarget::None,
        };
        let ranger_ticks = self.until(play, barracks, ranger, |play| {
            Ok(play.owned(RANGER, 0)?.len() == 1)
        })?;
        let production = ProductionSteps {
            dozer_ticks,
            barracks_ticks,
            ranger_ticks,
            money: [after_dozer, after_barracks, self.money(play)],
        };
        let fight = self.fight(play, barracks, ranger)?;
        Ok(SkirmishSteps { production, fight })
    }

    /// The fight in `play`: the barracks `barracks` trains `RANGERS` in all by `train`, one order
    /// a tick while its queue has room; they attack the bot's command center; the ticks of each
    /// part, the money left, how the match ended, and whether the bot's dozer's life is gone.
    fn fight(
        &self,
        play: &mut MatchPlay,
        barracks: StableId,
        train: OrdersAction,
    ) -> Result<FightSteps, Failure> {
        let [target] = play.owned(CENTER, 1)?[..] else {
            return Err(Failure::FightStart);
        };
        let [bot_dozer] = play.owned(DOZER, 1)?[..] else {
            return Err(Failure::FightStart);
        };
        let mut train_ticks = 0;
        while play.owned(RANGER, 0)?.len() < RANGERS {
            if train_ticks == STEP_LIMIT {
                return Err(Failure::ProductionStep(train));
            }
            let queued = MatchUnits::of(play.game())
                .unit(barracks)
                .get::<TrainQueue>()
                .map_or(0, |queue| queue.entries().len());
            if play.owned(RANGER, 0)?.len() + queued < RANGERS && queued < QUEUE {
                play.order(&[Order::one(barracks, train)]);
            }
            play.step();
            train_ticks += 1;
        }
        let money = self.money(play);
        let rangers = play.owned(RANGER, 0)?;
        let units = OrderUnits::new(&rangers).expect("the play's rangers");
        play.order(&[Order {
            units,
            action: OrdersAction::Attack { target }.into(),
        }]);
        for fight_ticks in 1..=STEP_LIMIT {
            play.step();
            let world = play.game();
            let end = world
                .match_entity()
                .and_then(|entity| world.get::<MatchEnd>(entity).copied());
            if let Some(end) = end {
                let bot_dozer = MatchUnits::of(world).unit(bot_dozer);
                return Ok(FightSteps {
                    train_ticks,
                    fight_ticks,
                    money,
                    result: end.result(),
                    killed: bot_dozer.get::<Life>().is_some_and(|life| life.is_empty()),
                });
            }
        }
        Err(Failure::NoEnd)
    }

    /// Orders `unit` to `action` in the tick `play` runs next, then runs ticks until `done`; the
    /// ticks it took, the order's own among them, or a failure past `STEP_LIMIT`.
    fn until(
        &self,
        play: &mut MatchPlay,
        unit: StableId,
        action: OrdersAction,
        done: impl Fn(&MatchPlay) -> Result<bool, Failure>,
    ) -> Result<u64, Failure> {
        play.order(&[Order::one(unit, action)]);
        for ticks in 1..=STEP_LIMIT {
            play.step();
            if done(play)? {
                return Ok(ticks);
            }
        }
        Err(Failure::ProductionStep(action))
    }

    /// The slot of `action` among the actions of the type `unit` in its slot kind `command`; a
    /// failure when the mode lacks the type or the action.
    fn slot(&self, unit: &'static str, action: &'static str) -> Result<u8, Failure> {
        let command = DeclaredName::new("command").expect("a declared name");
        let at = self
            .packages
            .content()
            .units
            .get(unit)
            .and_then(|held| held.slots.get(&command))
            .and_then(|slots| slots.iter().position(|held| held.as_str() == action))
            .ok_or(Failure::NoAction { unit, action })?;
        Ok(u8::try_from(at).expect("a unit's slots fit a u8"))
    }

    /// Player 1's money.
    fn money(&self, play: &MatchPlay) -> i64 {
        play.game()
            .player_resources(PlayerSlot::new(0))
            .expect("player 1 holds resources")
            .amount(self.money)
    }
}
