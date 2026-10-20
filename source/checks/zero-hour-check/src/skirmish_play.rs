use std::num::NonZeroU32;

use campfire_actions::{ActionTarget, TypeSlotsData};
use campfire_bots::{BotNames, BotScript, OrderScript};
use campfire_capabilities::Order;
use campfire_common::{DeclaredName, Seat};
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::{MatchEnd, OptionPicks, OptionValue};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_players::{OrderUnits, PlayersWorld};
use campfire_production::{Site, TrainQueue};
use campfire_projectiles::Projectile;
use campfire_protocol::{InputRules, SeatPlan};
use campfire_script::MatchNames;
use campfire_sim::StableId;
use campfire_units::{BuildTarget, GroundPoint, Life, UnitsWorld};
use campfire_values::ResourceId;

use crate::failure::Failure;
use crate::match_play::{CENTER, DOZER, MatchPlay};
use crate::pins::{BOT_TYPES, BotBase, CRUSADERS, FactorySteps, FightSteps, HZ, ProductionSteps};

/// The unit types the play makes beside the start's.
const BARRACKS: &str = "americabarracks";
const RANGER: &str = "americainfantryranger";
const FACTORY: &str = "americawarfactory";
const CRUSADER: &str = "americatankcrusader";

/// The most ticks the play waits for a step, past which it never ends.
const STEP_LIMIT: u64 = HZ * 60 * 5;

/// The most trains a factory queues, its `MaxQueueEntries`, which the play keeps it below.
const QUEUE: usize = 9;

/// Where the dozer places the barracks: 216 m along −z from `Player_1_Start`, clear of the
/// command center's box and of the cliffs, at America's `PlacementViewAngle` of −135°.
const BARRACKS_AT: [i64; 3] = [1005, -800, -135];

/// The buildings the dozer builds after the barracks, each the prerequisite of the next, at its
/// place: the power plant 150 m along −x from `Player_1_Start`, the supply center 150 m along
/// +x, and the war factory 200 m past the barracks along −z, each clear of the cliffs and of the
/// others, at America's angle.
const FACTORY_CHAIN: [Building; 3] = [
    Building {
        unit: "americapowerplant",
        action: "build_americapowerplant",
        at: [855, -584, -135],
    },
    Building {
        unit: "americasupplycenter",
        action: "build_americasupplycenter",
        at: [1155, -584, -135],
    },
    Building {
        unit: FACTORY,
        action: "build_americawarfactory",
        at: [1005, -1000, -135],
    },
];

/// The builder bot's orders, which the play's bot plays and a host gives a local server's bot
/// (09-zero-hour.md, A bot that builds and does not fight).
const BUILDER: &str = include_str!("../bots/builder.toml");

/// The money every player starts with, the most of Zero Hour's choices, which the session's host
/// picks (09-zero-hour.md, The import writes Zero Hour's money as an option).
const START_MONEY: &str = "start_money=50000";

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

/// A building the dozer builds: its unit type, its build action, and its place, `[x, z, angle]`.
#[derive(Debug, Clone, Copy)]
struct Building {
    unit: &'static str,
    action: &'static str,
    at: [i64; 3],
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
        failures: &mut Vec<Failure>,
    ) -> Option<SkirmishSteps> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let options = &packages.data().options;
        let picks = match OptionPicks::named(options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(Failure::Options(error));
                return None;
            }
        };
        let script = OrderScript::parse(BUILDER).map_err(Failure::BotOrders);
        let bot = script.and_then(|script| {
            let books = packages.name_books();
            let names = BotNames::new(&books);
            BotScript::new(&script, &names).map_err(Failure::BotNames)
        });
        let bot = match bot {
            Ok(bot) => bot,
            Err(failure) => {
                failures.push(failure);
                return None;
            }
        };
        let plan = [SeatPlan::Player, SeatPlan::Bot];
        let session =
            FixedSession::planned(packages, rate, InputRules::ROOMY, plan).picked(picks.places());
        let mut playing = MatchPlay::new(session.start());
        let names = playing.game().resource::<MatchNames>();
        let Some(money) = names.named::<ResourceId>("money") else {
            failures.push(Failure::NoMoney);
            playing.end();
            return None;
        };
        let play = Play {
            packages: session.packages(),
            money,
        };
        let played = play.steps(&mut playing, bot);
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
    /// The production steps in `play`, after its start's tick, then the factory's, then the
    /// fight.
    fn steps(&self, play: &mut MatchPlay, bot: BotScript) -> Result<SkirmishSteps, Failure> {
        play.step();
        play.drive_bot(1, bot)?;
        let [center] = play.owned(CENTER, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        let [dozer] = play.owned(DOZER, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        // The center trains a dozer: its second, once its time ends.
        let train = OrdersDirective::Slot {
            slot: self.slot(CENTER, "train_americavehicledozer")?,
            target: ActionTarget::None,
        };
        let dozer_ticks = self.until(play, center, train, |play| {
            Ok(play.owned(DOZER, 0)?.len() == 2)
        })?;
        let after_dozer = self.money(play);
        // The start dozer places the barracks as the order applies, and builds it whole.
        let [x, z, angle] = BARRACKS_AT.map(Num::int);
        let build = OrdersDirective::Build {
            slot: self.slot(DOZER, "build_americabarracks")?,
            target: BuildTarget::Point {
                at: GroundPoint { x, z },
                angle,
            },
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
        let ranger = OrdersDirective::Slot {
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
        let factory = self.factory(play, dozer)?;
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

    /// The dozer `dozer` builds each of `FACTORY_CHAIN` whole in `play`, one after another; the
    /// ticks of each and the money left.
    fn factory(&self, play: &mut MatchPlay, dozer: StableId) -> Result<FactorySteps, Failure> {
        let mut ticks = [0; FACTORY_CHAIN.len()];
        for (building, ticks) in FACTORY_CHAIN.iter().zip(&mut ticks) {
            let [x, z, angle] = building.at.map(Num::int);
            let build = OrdersDirective::Build {
                slot: self.slot(DOZER, building.action)?,
                target: BuildTarget::Point {
                    at: GroundPoint { x, z },
                    angle,
                },
            };
            *ticks = self.until(play, dozer, build, |play| {
                let built = play.owned(building.unit, 0)?;
                let units = MatchUnits::of(play.game());
                Ok(built.len() == 1 && !units.unit(built[0]).contains::<Site>())
            })?;
        }
        Ok(FactorySteps {
            ticks,
            money: self.money(play),
        })
    }

    /// Player 1's war factory in `play` trains `CRUSADERS`, one order a tick while its queue has
    /// room; the ticks the trains took.
    fn train_crusaders(&self, play: &mut MatchPlay) -> Result<u64, Failure> {
        let [factory] = play.owned(FACTORY, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        let train = OrdersDirective::Slot {
            slot: self.slot(FACTORY, "train_americatankcrusader")?,
            target: ActionTarget::None,
        };
        let crusaders = usize::from(CRUSADERS);
        let mut train_ticks = 0;
        while play.owned(CRUSADER, 0)?.len() < crusaders {
            if train_ticks == STEP_LIMIT {
                return Err(Failure::ProductionStep(train));
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
    fn bot_built(play: &MatchPlay) -> Result<[usize; BOT_TYPES.len()], Failure> {
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
    fn attack(&self, play: &mut MatchPlay, train_ticks: u64) -> Result<FightSteps, Failure> {
        let [target] = play.owned(CENTER, 1)?[..] else {
            return Err(Failure::FightStart);
        };
        let money = self.money(play);
        let crusaders = play.owned(CRUSADER, 0)?;
        let units = OrderUnits::new(&crusaders).expect("the play's crusaders");
        play.order(&[Order {
            units,
            directive: OrdersDirective::Attack { target }.into(),
        }]);
        let full = Self::life(play, target);
        let (mut shot_ticks, mut hit_ticks) = (None, None);
        for fight_ticks in 1..=STEP_LIMIT {
            play.step();
            let world = play.game();
            let units = MatchUnits::of(world);
            if shot_ticks.is_none() && units.all().any(|(_, unit)| unit.contains::<Projectile>()) {
                shot_ticks = Some(fight_ticks);
            }
            if hit_ticks.is_none() && Self::life(play, target) < full {
                hit_ticks = Some(fight_ticks);
            }
            let life = Self::life(play, target);
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
        Err(Failure::NoEnd)
    }

    /// The life of `unit` in `play`, none once it is gone.
    fn life(play: &MatchPlay, unit: StableId) -> Option<Num> {
        let units = MatchUnits::of(play.game());
        let unit = units.all().find(|(id, _)| *id == unit)?.1;
        unit.get::<Life>().map(|life| life.current())
    }

    /// Orders `unit` to `directive` in the tick `play` runs next, then runs ticks until `done`; the
    /// ticks it took, the order's own among them, or a failure past `STEP_LIMIT`.
    fn until(
        &self,
        play: &mut MatchPlay,
        unit: StableId,
        directive: OrdersDirective,
        done: impl Fn(&MatchPlay) -> Result<bool, Failure>,
    ) -> Result<u64, Failure> {
        play.order(&[Order::one(unit, directive)]);
        for ticks in 1..=STEP_LIMIT {
            play.step();
            if done(play)? {
                return Ok(ticks);
            }
        }
        Err(Failure::ProductionStep(directive))
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
            .and_then(|held| held.section::<TypeSlotsData>()?.0.get(&command))
            .and_then(|slots| slots.iter().position(|held| held.as_str() == action))
            .ok_or(Failure::NoAction { unit, action })?;
        Ok(u8::try_from(at).expect("a unit's slots fit a u8"))
    }

    /// Player 1's money.
    fn money(&self, play: &MatchPlay) -> i64 {
        play.game()
            .player_resources(Seat::new(0))
            .expect("player 1 holds resources")
            .amount(self.money)
    }
}
