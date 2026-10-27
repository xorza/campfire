use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_airfields::Sortie;
use campfire_capabilities::Order;
use campfire_combat::ClipShots;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::{OptionPicks, OptionValue, PlayerSlotPick};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_protocol::PlayerSlotPlan;
use campfire_units::{Destination, GroundPoint};

use crate::failure::CheckFailure;
use crate::match_play::{self, MatchPlay, PlayTools, START_MONEY, STEP_LIMIT};
use crate::pins::JetTicks;

/// America's airfield, its jet and the jet's train, and GLA's command center, the target.
const AIRFIELD: &str = "americaairfield";
const RAPTOR: &str = "americajetraptor";
const TRAIN: &str = "train_americajetraptor";
const TARGET: &str = "glacommandcenter";

/// A Raptor's sortie on Tournament Desert: player 1, America, from its base
/// start, and player 2, GLA, from its standard start, ordered nothing but the play's orders.
/// Player 1's airfield trains a Raptor, made at its place; sent over GLA's command center, it takes
/// off down its runway and flies there, and ordered to attack the center, as it sees it, strikes
/// it, goes home empty, lands, taxis back to its place and reloads its clip there.
#[derive(Debug)]
pub(crate) struct JetPlay;

impl JetPlay {
    /// Plays the sortie in a match of `packages` at `rate` ticks a second: the ticks of its steps
    /// and the target's life after them; a failure goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<JetTicks> {
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
            "1:start=standard",
        ]
        .map(|pick| pick.parse().expect("the play's pick"))
        .into();
        let session = FixedSession::planned(packages, [PlayerSlotPlan::Player; 2])
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&player_slot_picks);
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

    /// The steps in `play` of `packages`, after its start's tick.
    fn steps(play: &mut MatchPlay<'_>, packages: &ModePackages) -> Result<JetTicks, CheckFailure> {
        play.step();
        let [airfield, ..] = play.owned(AIRFIELD, 0)?[..] else {
            return Err(CheckFailure::NoJetStep("an airfield"));
        };
        let [target, ..] = play.owned(TARGET, 1)?[..] else {
            return Err(CheckFailure::NoJetStep("a target"));
        };
        let train = OrdersDirective::Slot {
            slot: match_play::slot(packages, AIRFIELD, TRAIN)?,
            target: ActionTarget::None,
        };
        let trained = play.until(airfield, train, |play| {
            Ok(!play.owned(RAPTOR, 0)?.is_empty())
        })?;
        let [raptor] = play.owned(RAPTOR, 0)?[..] else {
            return Err(CheckFailure::NoJetStep("a Raptor"));
        };
        // It flies over the target first, as an attack aims only at what its team sees.
        let at = MatchUnits::of(play.game()).position(target);
        let fly = OrdersDirective::Move {
            to: GroundPoint { x: at.x, z: at.z },
        };
        play.order(&[Order::one(raptor, fly)]);
        let sortie = |play: &MatchPlay<'_>| {
            let units = MatchUnits::of(play.game());
            units.unit(raptor).get::<Sortie>().copied()
        };
        let took_off = Self::until(play, "its takeoff", |play| {
            matches!(sortie(play), Some(Sortie::Flying { .. }))
        })?;
        let over = Self::until(play, "the target", |play| {
            let units = MatchUnits::of(play.game());
            units
                .unit(raptor)
                .get::<Destination>()
                .is_some_and(|to| to.get().is_none())
        })?;
        let life = play.life(target);
        play.order(&[Order::one(raptor, OrdersDirective::Attack { target })]);
        let hit = Self::until(play, "its first hit", |play| play.life(target) != life)?;
        let home = Self::until(play, "its way home", |play| {
            sortie(play) == Some(Sortie::Home)
        })?;
        let parked = Self::until(play, "its landing", |play| {
            matches!(sortie(play), Some(Sortie::Reload { .. } | Sortie::Parked))
        })?;
        let reloaded = Self::until(play, "its reload", |play| {
            let units = MatchUnits::of(play.game());
            let full = units.unit(raptor).get::<ClipShots>() == Some(&ClipShots::default());
            full && sortie(play) == Some(Sortie::Parked)
        })?;
        let target_life = play.life(target).map_or(0, Num::floor);
        Ok(JetTicks {
            trained,
            took_off,
            over,
            hit,
            home,
            parked,
            reloaded,
            target_life,
        })
    }

    /// The ticks `play` runs until `done` holds, at most `STEP_LIMIT`; a failure of `step` past
    /// it.
    fn until(
        play: &mut MatchPlay<'_>,
        step: &'static str,
        done: impl Fn(&MatchPlay<'_>) -> bool,
    ) -> Result<u64, CheckFailure> {
        for ticks in 1..=STEP_LIMIT {
            play.step();
            if done(play) {
                return Ok(ticks);
            }
        }
        Err(CheckFailure::NoJetStep(step))
    }
}
