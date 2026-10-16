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
use campfire_projectiles::Projectile;
use campfire_protocol::{InputRules, SlotPlan};
use campfire_sim::StableId;
use campfire_stats::Life;
use campfire_units::UnitsWorld;
use campfire_values::{DeclaredName, Names, ResourceId};

use crate::failure::Failure;
use crate::match_play::{CENTER, DOZER, MatchPlay};
use crate::pins::{FactorySteps, FightSteps, ProductionSteps};

/// The unit types the play makes beside the start's.
const BARRACKS: &str = "americabarracks";
const RANGER: &str = "americainfantryranger";
const FACTORY: &str = "americawarfactory";
const CRUSADER: &str = "americatankcrusader";

/// The most ticks the play waits for a step, past which it never ends.
const STEP_LIMIT: u64 = 30 * 60 * 5;

/// How many crusaders the war factory trains and sends to fight, as the money left pays for.
const CRUSADERS: usize = 3;

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

/// The skirmish on Tournament Desert against an idle bot (designs 18 and 19;
/// 20-zero-hour-projectiles.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons): player 1's command center trains a dozer, its start dozer builds a barracks,
/// and the barracks trains a ranger; then the dozer builds a power plant, a supply center and a
/// war factory, the factory trains `CRUSADERS`, and their shells destroy the bot's command
/// center, which ends the match.
#[derive(Debug)]
pub(crate) struct SkirmishPlay;

/// What the play measures: its production steps, its factory's, and its fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkirmishSteps {
    pub(crate) production: ProductionSteps,
    pub(crate) factory: FactorySteps,
    pub(crate) fight: FightSteps,
}

/// A building the dozer builds: its unit type, its build action, and its place, `[x, z, angle]`.
#[derive(Debug, Clone, Copy)]
struct Building {
    unit: &'static str,
    action: &'static str,
    at: [i64; 3],
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
    /// The production steps in `play`, after its start's tick, then the factory's, then the
    /// fight.
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
        let factory = self.factory(play, dozer)?;
        let fight = self.fight(play)?;
        Ok(SkirmishSteps {
            production,
            factory,
            fight,
        })
    }

    /// The dozer `dozer` builds each of `FACTORY_CHAIN` whole in `play`, one after another; the
    /// ticks of each and the money left.
    fn factory(&self, play: &mut MatchPlay, dozer: StableId) -> Result<FactorySteps, Failure> {
        let mut ticks = [0; FACTORY_CHAIN.len()];
        for (building, ticks) in FACTORY_CHAIN.iter().zip(&mut ticks) {
            let [x, z, angle] = building.at.map(Num::int);
            let build = OrdersAction::Build {
                slot: self.slot(DOZER, building.action)?,
                target: BuildTarget::Point { x, z, angle },
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

    /// The fight in `play`: player 1's war factory trains `CRUSADERS`, one order a tick while its
    /// queue has room; they attack the bot's command center; the ticks of the trains, from
    /// the attack's order to the first shell's launch and to the first damage to the center, and
    /// to the match's end; the money left, how the match ended, and whether the bot's dozer's
    /// life is gone.
    fn fight(&self, play: &mut MatchPlay) -> Result<FightSteps, Failure> {
        let [factory] = play.owned(FACTORY, 0)?[..] else {
            return Err(Failure::ProductionStart);
        };
        let [target] = play.owned(CENTER, 1)?[..] else {
            return Err(Failure::FightStart);
        };
        let [bot_dozer] = play.owned(DOZER, 1)?[..] else {
            return Err(Failure::FightStart);
        };
        let train = OrdersAction::Slot {
            slot: self.slot(FACTORY, "train_americatankcrusader")?,
            target: ActionTarget::None,
        };
        let mut train_ticks = 0;
        while play.owned(CRUSADER, 0)?.len() < CRUSADERS {
            if train_ticks == STEP_LIMIT {
                return Err(Failure::ProductionStep(train));
            }
            let queued = MatchUnits::of(play.game())
                .unit(factory)
                .get::<TrainQueue>()
                .map_or(0, |queue| queue.entries().len());
            if play.owned(CRUSADER, 0)?.len() + queued < CRUSADERS && queued < QUEUE {
                play.order(&[Order::one(factory, train)]);
            }
            play.step();
            train_ticks += 1;
        }
        let money = self.money(play);
        let crusaders = play.owned(CRUSADER, 0)?;
        let units = OrderUnits::new(&crusaders).expect("the play's crusaders");
        play.order(&[Order {
            units,
            action: OrdersAction::Attack { target }.into(),
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
            let end = world
                .match_entity()
                .and_then(|entity| world.get::<MatchEnd>(entity).copied());
            if let Some(end) = end {
                let bot_dozer = units.unit(bot_dozer);
                return Ok(FightSteps {
                    train_ticks,
                    shot_ticks,
                    hit_ticks,
                    fight_ticks,
                    money,
                    result: end.result(),
                    killed: bot_dozer.get::<Life>().is_some_and(|life| life.is_empty()),
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
