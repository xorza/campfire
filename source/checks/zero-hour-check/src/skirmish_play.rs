use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_bots::BotScript;
use campfire_capabilities::Order;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::{MatchEnd, OptionPicks, OptionValue};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_players::OrderUnits;
use campfire_production::{Site, TrainQueue};
use campfire_projectiles::Projectile;
use campfire_protocol::{InputRules, PlayerSlotPlan};
use campfire_units::UnitsWorld;
use campfire_values::ResourceId;

use crate::failure::CheckFailure;
use crate::match_play::{
    self, BuildingDozer, CENTER, DOZER, MatchPlay, PlannedBuilding, PlayTools, START_MONEY,
    STEP_LIMIT,
};
use crate::pins::{BOT_TYPES, BotBase, CRUSADERS, FactorySteps, FightSteps, ProductionSteps};

/// The unit types the play makes beside the start's.
const BARRACKS: &str = "americabarracks";
const RANGER: &str = "americainfantryranger";
const FACTORY: &str = "americawarfactory";
const CRUSADER: &str = "americatankcrusader";

/// The most trains a factory queues, its `MaxQueueEntries`, which the play keeps it below.
const QUEUE: usize = 9;

/// The barracks, which the dozer places 216 m along −z from `Player_1_Start`, clear of the
/// command center's box and of the cliffs, at America's `PlacementViewAngle` of −135°.
const BARRACKS_BUILDING: PlannedBuilding = PlannedBuilding {
    unit: BARRACKS,
    action: "build_americabarracks",
    at: [1005, -800, -135],
};

/// The buildings the dozer builds after the barracks, each the prerequisite of the next, at its
/// place: the power plant 150 m along −x from `Player_1_Start`, the supply center 150 m along
/// +x, and the war factory 200 m past the barracks along −z, each clear of the cliffs and of the
/// others, at America's angle.
const FACTORY_CHAIN: [PlannedBuilding; 3] = [
    PlannedBuilding {
        unit: "americapowerplant",
        action: "build_americapowerplant",
        at: [855, -584, -135],
    },
    PlannedBuilding {
        unit: "americasupplycenter",
        action: "build_americasupplycenter",
        at: [1155, -584, -135],
    },
    PlannedBuilding {
        unit: FACTORY,
        action: "build_americawarfactory",
        at: [1005, -1000, -135],
    },
];

/// The builder bot's orders, which the play's bot plays, a fixture of the check
/// (09-zero-hour.md, A bot that builds and does not fight; 04-capabilities/control.md, Order
/// scripts stay as test fixtures).
const BUILDER: &str = include_str!("../fixtures/builder.toml");

/// The skirmish on Tournament Desert of `START_MONEY` against the builder bot (designs 18 and 19;
/// 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons): player 1's command center trains a dozer, its start dozer builds a barracks,
/// and the barracks trains a ranger; then the dozer builds a power plant, a supply center and a
/// war factory, the factory trains `CRUSADERS`, and their shells destroy the bot's command
/// center, which ends no match, as the bot's other buildings count for its victory. The bot
/// builds its base and trains its units meanwhile, and orders no attack.
#[derive(Debug)]
pub(crate) struct SkirmishPlay;

/// What the play measures: its production steps, its factory's, its fight, and the bot's base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkirmishSteps {
    pub(crate) production: ProductionSteps,
    pub(crate) factory: FactorySteps,
    pub(crate) fight: FightSteps,
    pub(crate) bot: BotBase,
}

/// What the play reads of its match: player 1's money, which the match names, and the slots of
/// its types' actions.
#[derive(Debug)]
struct Play<'a> {
    packages: &'a ModePackages,
    money: ResourceId,
}

impl SkirmishPlay {
    /// Plays the skirmish in a match of `packages` at `rate` ticks a second, player 1 against the
    /// builder bot: each production step's ticks and the money left after each, the fight's, and
    /// the bot's base; a failure for a step that does not end, a unit the play lacks, or an
    /// option or a bot's script the mode does not take, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<SkirmishSteps> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let options = &packages.data().options;
        let picks = match OptionPicks::named(options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let bot = match match_play::bot_script(&packages, BUILDER) {
            Ok(bot) => bot,
            Err(failure) => {
                failures.push(failure);
                return None;
            }
        };
        let plan = [PlayerSlotPlan::Player, PlayerSlotPlan::Bot];
        let session = FixedSession::planned(packages, InputRules::ROOMY, plan)
            .rate(rate)
            .picked(picks.places())
            // America, whose start the play reads, in place of a faction drawn (09-zero-hour.md,
            // No pick is Random).
            .each_picked("faction", "america");
        let mut playing = MatchPlay::new(session.start(), session.packages(), tools);
        let Some(money) = MatchUnits::of(playing.game()).resource("money") else {
            failures.push(CheckFailure::NoMoney);
            failures.extend(playing.end().err());
            return None;
        };
        let play = Play {
            packages: session.packages(),
            money,
        };
        let played = play.steps(&mut playing, bot);
        let ended = playing.end();
        match played.and_then(|played| ended.map(|()| played)) {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }
}

impl Play<'_> {
    /// The production steps in `play`, after its start's tick, then the factory's, then the
    /// fight.
    fn steps(
        &self,
        play: &mut MatchPlay<'_>,
        bot: BotScript,
    ) -> Result<SkirmishSteps, CheckFailure> {
        play.step();
        let home = play.center(1)?;
        play.drive_bot(1, bot, home);
        let [center] = play.owned(CENTER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let [dozer] = play.owned(DOZER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        // The center trains a dozer: its second, once its time ends.
        let train = OrdersDirective::Slot {
            slot: match_play::slot(self.packages, CENTER, "train_americavehicledozer")?,
            target: ActionTarget::None,
        };
        let dozer_ticks = play.until(center, train, |play| Ok(play.owned(DOZER, 0)?.len() == 2))?;
        let after_dozer = play.money(self.money);
        // The start dozer places the barracks as the order applies, and builds it whole.
        let builder = BuildingDozer {
            unit: DOZER,
            id: dozer,
        };
        let barracks_ticks = play.build(self.packages, builder, BARRACKS_BUILDING)?;
        let after_barracks = play.money(self.money);
        let [barracks] = play.owned(BARRACKS, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        // The barracks trains a ranger, which needs the barracks whole.
        let ranger = OrdersDirective::Slot {
            slot: match_play::slot(self.packages, BARRACKS, "train_americainfantryranger")?,
            target: ActionTarget::None,
        };
        let ranger_ticks = play.until(barracks, ranger, |play| {
            Ok(play.owned(RANGER, 0)?.len() == 1)
        })?;
        let production = ProductionSteps {
            dozer_ticks,
            barracks_ticks,
            ranger_ticks,
            money: [after_dozer, after_barracks, play.money(self.money)],
        };
        let factory = self.factory(play, builder)?;
        let train_ticks = self.train_crusaders(play)?;
        let built = Self::bot_built(play)?;
        let fight = self.attack(play, train_ticks)?;
        let farthest = play.bot_farthest().expect("the play drives its bot");
        let bot = BotBase {
            built,
            farthest_m: farthest.floor(),
        };
        Ok(SkirmishSteps {
            production,
            factory,
            fight,
            bot,
        })
    }

    /// The dozer `builder` builds each of `FACTORY_CHAIN` whole in `play`, one after another; the
    /// ticks of each and the money left.
    fn factory(
        &self,
        play: &mut MatchPlay<'_>,
        builder: BuildingDozer,
    ) -> Result<FactorySteps, CheckFailure> {
        let mut ticks = [0; FACTORY_CHAIN.len()];
        for (&building, ticks) in FACTORY_CHAIN.iter().zip(&mut ticks) {
            *ticks = play.build(self.packages, builder, building)?;
        }
        Ok(FactorySteps {
            ticks,
            money: play.money(self.money),
        })
    }

    /// Player 1's war factory in `play` trains `CRUSADERS`, one order a tick while its queue has
    /// room; the ticks the trains took.
    fn train_crusaders(&self, play: &mut MatchPlay<'_>) -> Result<u64, CheckFailure> {
        let [factory] = play.owned(FACTORY, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let train = OrdersDirective::Slot {
            slot: match_play::slot(self.packages, FACTORY, "train_americatankcrusader")?,
            target: ActionTarget::None,
        };
        let crusaders = usize::from(CRUSADERS);
        let mut train_ticks = 0;
        while play.owned(CRUSADER, 0)?.len() < crusaders {
            if train_ticks == STEP_LIMIT {
                return Err(CheckFailure::ProductionStep(train));
            }
            let queued = MatchUnits::of(play.game())
                .unit(factory)
                .get::<TrainQueue>()
                .map_or(0, |queue| queue.entries().len());
            if play.owned(CRUSADER, 0)?.len() + queued < crusaders && queued < QUEUE {
                play.order(&[Order::one(factory, train)]);
            }
            play.step();
            train_ticks += 1;
        }
        Ok(train_ticks)
    }

    /// How many living units of each of `BOT_TYPES` the bot owns in `play`, whole, as a site
    /// counts for no building.
    fn bot_built(play: &MatchPlay<'_>) -> Result<[usize; BOT_TYPES.len()], CheckFailure> {
        let units = MatchUnits::of(play.game());
        let mut built = [0; BOT_TYPES.len()];
        for (count, name) in built.iter_mut().zip(BOT_TYPES) {
            let owned = play.owned(name, 1)?;
            *count = owned
                .iter()
                .filter(|&&id| !units.unit(id).contains::<Site>())
                .count();
        }
        Ok(built)
    }

    /// The fight in `play`, after player 1's crusaders were trained in `train_ticks`: they attack
    /// the bot's command center; the ticks from the attack's order to the first shell's launch,
    /// to the first damage to the center, and to its death; the money left, and how the match
    /// ended by then, if it did, as the bot's other buildings count for its victory.
    fn attack(
        &self,
        play: &mut MatchPlay<'_>,
        train_ticks: u64,
    ) -> Result<FightSteps, CheckFailure> {
        let [target] = play.owned(CENTER, 1)?[..] else {
            return Err(CheckFailure::FightStart);
        };
        let money = play.money(self.money);
        let crusaders = play.owned(CRUSADER, 0)?;
        let units = OrderUnits::new(&crusaders).expect("the play's crusaders");
        play.order(&[Order {
            units,
            directive: OrdersDirective::Attack { target }.into(),
        }]);
        let full = play.life(target);
        let (mut shot_ticks, mut hit_ticks) = (None, None);
        for fight_ticks in 1..=STEP_LIMIT {
            play.step();
            let world = play.game();
            let units = MatchUnits::of(world);
            if shot_ticks.is_none() && units.all().any(|(_, unit)| unit.contains::<Projectile>()) {
                shot_ticks = Some(fight_ticks);
            }
            if hit_ticks.is_none() && play.life(target) < full {
                hit_ticks = Some(fight_ticks);
            }
            let life = play.life(target);
            if life.is_none_or(|life| life == Num::ZERO) {
                let end = world
                    .match_entity()
                    .and_then(|entity| world.get::<MatchEnd>(entity).copied());
                return Ok(FightSteps {
                    train_ticks,
                    shot_ticks,
                    hit_ticks,
                    fight_ticks,
                    money,
                    result: end.map(MatchEnd::result),
                });
            }
        }
        Err(CheckFailure::NoEnd)
    }
}
