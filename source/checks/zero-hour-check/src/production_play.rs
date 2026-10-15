use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_capabilities::Order;
use campfire_common::{PlayerSlot, Tick};
use campfire_harness::{FixedMatch, FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_orders::OrdersAction;
use campfire_package::ModePackages;
use campfire_players::PlayersWorld;
use campfire_production::{BuildTarget, Site};
use campfire_runner::SessionWorld;
use campfire_sim::StableId;
use campfire_units::Units;
use campfire_values::{DeclaredName, Names, ResourceId, UnitType};

use crate::failure::{Failure, ProductionSteps};

/// The unit types the play makes, and its producers'.
const CENTER: &str = "americacommandcenter";
const DOZER: &str = "americavehicledozer";
const BARRACKS: &str = "americabarracks";
const RANGER: &str = "americainfantryranger";

/// The most ticks the play waits for a step, past which it never ends.
const STEP_LIMIT: u64 = 30 * 60 * 5;

/// Where the dozer places the barracks: 216 m along −z from `Player_1_Start`, clear of the
/// command center's box and of the cliffs, at America's `PlacementViewAngle` of −135°.
const BARRACKS_AT: [i64; 3] = [1005, -800, -135];

/// The skirmish's production on Tournament Desert (design 18): player 1's command center trains
/// a dozer, its start dozer builds a barracks, and the barracks trains a ranger.
#[derive(Debug)]
pub(crate) struct ProductionPlay;

/// What the play reads of its match: player 1's money, and the slots of its types' actions.
#[derive(Debug)]
struct Play<'a> {
    packages: &'a ModePackages,
    money: ResourceId,
}

impl ProductionPlay {
    /// Plays the three steps in a match of `packages` at `rate` ticks a second: each step's ticks,
    /// and the money left after each; a failure for a step that does not end, or a unit the play
    /// lacks, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<Failure>,
    ) -> Option<ProductionSteps> {
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
        let session = FixedSession::new(packages, rate, 1);
        let mut fixed = session.start();
        let play = Play {
            packages: session.packages(),
            money,
        };
        let played = play.steps(&mut fixed);
        fixed.end();
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
    /// The three steps in `fixed`, after its start's tick.
    fn steps(&self, fixed: &mut FixedMatch) -> Result<ProductionSteps, Failure> {
        let mut tick = Tick::new(0);
        Play::step(fixed, &mut tick);
        let [center] = self.owned(fixed, CENTER)[..] else {
            return Err(Failure::ProductionStart);
        };
        let [dozer] = self.owned(fixed, DOZER)[..] else {
            return Err(Failure::ProductionStart);
        };
        // The center trains a dozer: its second, once its time ends.
        let train = OrdersAction::Slot {
            slot: self.slot(CENTER, "train_americavehicledozer"),
            target: ActionTarget::None,
        };
        let dozer_ticks = self.until(fixed, &mut tick, center, train, |play, fixed| {
            play.owned(fixed, DOZER).len() == 2
        })?;
        let after_dozer = self.money(fixed);
        // The start dozer places the barracks as the order applies, and builds it whole.
        let [x, z, angle] = BARRACKS_AT.map(Num::int);
        let build = OrdersAction::Build {
            slot: self.slot(DOZER, "build_americabarracks"),
            target: BuildTarget::Point { x, z, angle },
        };
        let barracks_ticks = self.until(fixed, &mut tick, dozer, build, |play, fixed| {
            let built = play.owned(fixed, BARRACKS);
            let units = MatchUnits::of(fixed.game());
            built.len() == 1 && !units.unit(built[0]).contains::<Site>()
        })?;
        let after_barracks = self.money(fixed);
        let [barracks] = self.owned(fixed, BARRACKS)[..] else {
            return Err(Failure::ProductionStart);
        };
        // The barracks trains a ranger, which needs the barracks whole.
        let ranger = OrdersAction::Slot {
            slot: self.slot(BARRACKS, "train_americainfantryranger"),
            target: ActionTarget::None,
        };
        let ranger_ticks = self.until(fixed, &mut tick, barracks, ranger, |play, fixed| {
            play.owned(fixed, RANGER).len() == 1
        })?;
        Ok(ProductionSteps {
            dozer_ticks,
            barracks_ticks,
            ranger_ticks,
            money: [after_dozer, after_barracks, self.money(fixed)],
        })
    }

    /// Orders `unit` to `action` in the tick `tick`, then runs ticks until `done`; the ticks it
    /// took, the order's own among them, or a failure past `STEP_LIMIT`.
    fn until(
        &self,
        fixed: &mut FixedMatch,
        tick: &mut Tick,
        unit: StableId,
        action: OrdersAction,
        done: impl Fn(&Play<'_>, &FixedMatch) -> bool,
    ) -> Result<u64, Failure> {
        fixed.send(0, *tick, &Order::payload(&[Order::one(unit, action)]));
        for ticks in 1..=STEP_LIMIT {
            Play::step(fixed, tick);
            if done(self, fixed) {
                return Ok(ticks);
            }
        }
        Err(Failure::ProductionStep(action))
    }

    /// Runs the tick `tick`, and moves it on.
    fn step(fixed: &mut FixedMatch, tick: &mut Tick) {
        fixed.game_mut().run_tick_alone();
        *tick = Tick::new(tick.get() + 1);
    }

    /// The living units player 1 owns of the type `name`, by stable id.
    fn owned(&self, fixed: &FixedMatch, name: &str) -> Vec<StableId> {
        let world = fixed.game();
        let typed: UnitType = Units::type_named(world, name).expect("a type the play makes");
        MatchUnits::of(world)
            .all()
            .filter(|(_, unit)| {
                unit.get::<UnitType>() == Some(&typed) && MatchUnits::owned_by(unit, 0)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// The slot of `action` among the actions of the type `unit` in its slot kind `command`.
    fn slot(&self, unit: &str, action: &str) -> u8 {
        let command = DeclaredName::new("command").expect("a declared name");
        let at = self.packages.content().units[unit].slots[&command]
            .iter()
            .position(|held| held.as_str() == action)
            .expect("the play's actions are in their units' slots");
        u8::try_from(at).expect("a unit's slots fit a u8")
    }

    /// Player 1's money.
    fn money(&self, fixed: &FixedMatch) -> i64 {
        fixed
            .game()
            .player_resources(PlayerSlot::new(0))
            .expect("player 1 holds resources")
            .amount(self.money)
    }
}
