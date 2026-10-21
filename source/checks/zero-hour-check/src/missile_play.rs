use std::num::NonZeroU32;

use campfire_actions::ActionTarget;
use campfire_harness::MatchUnits;
use campfire_math::Num;
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_projectiles::{Flight, Projectile};

use crate::failure::CheckFailure;
use crate::match_play::{
    self, AgainstAmerica, BuildingDozer, CHINA_DOZER, MatchPlay, PlannedBuilding, STEP_LIMIT,
};
use crate::pins::MissileTicks;

/// China's barracks, 216 m along −z from `Player_1_Start`, where the skirmish play places its
/// own, at China's `PlacementViewAngle` of −45°.
const BARRACKS: PlannedBuilding = PlannedBuilding {
    unit: "chinabarracks",
    action: "build_chinabarracks",
    at: [1005, -800, -45],
};

/// China's Tank Hunter.
const TANK_HUNTER: &str = "chinainfantrytankhunter";

/// How far south of the Tank Hunter the bot's dozer drives, in meters: within the 175 m its
/// missiles reach.
const SHORT_OF_THE_HUNTER: Num = Num::int(120);

/// A Tank Hunter's missiles on Tournament Desert (09-zero-hour.md, A missile converts to a steered
/// projectile): player 1, China, builds a barracks and trains a Tank Hunter, and the bot, America,
/// drives its dozer from its start to a point within the hunter's reach, where the hunter, ordered
/// nothing, fires steered missiles at it, and kills it.
#[derive(Debug)]
pub(crate) struct MissilePlay;

impl MissilePlay {
    /// Plays the missiles in a match of `packages` at `rate` ticks a second: the barracks' ticks
    /// and the Tank Hunter's, and the ticks from the dozer's order to the first steered missile,
    /// to its first damage and to its death; a failure for a step that does not end, a unit or an
    /// action the play lacks, an option the mode does not take, or damage that no steered missile
    /// came before, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<MissileTicks> {
        AgainstAmerica::CHINA
            .play(packages, rate, Self::steps)
            .map_err(|failure| failures.push(failure))
            .ok()
    }

    /// The steps in `play` of `packages`, after its start's tick.
    fn steps(play: &mut MatchPlay, packages: &ModePackages) -> Result<MissileTicks, CheckFailure> {
        play.step();
        let [dozer] = play.owned(CHINA_DOZER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let builder = BuildingDozer {
            unit: CHINA_DOZER,
            id: dozer,
        };
        let built = play.build(packages, builder, BARRACKS)?;
        let [barracks] = play.owned(BARRACKS.unit, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let train = OrdersDirective::Slot {
            slot: match_play::slot(packages, BARRACKS.unit, "train_chinainfantrytankhunter")?,
            target: ActionTarget::None,
        };
        let trained = play.until(barracks, train, |play| {
            Ok(play.owned(TANK_HUNTER, 0)?.len() == 1)
        })?;
        let [hunter] = play.owned(TANK_HUNTER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let enemy = play.drive_bot_dozer(packages, hunter, SHORT_OF_THE_HUNTER)?;
        let full = play.life(enemy);
        let mut shot = None;
        let mut hit = None;
        for ticks in 1..=STEP_LIMIT {
            play.step();
            let steered = MatchUnits::of(play.game()).all().any(|(_, unit)| {
                unit.get::<Projectile>()
                    .is_some_and(|projectile| matches!(projectile.flight(), Flight::Steered { .. }))
            });
            if steered {
                shot.get_or_insert(ticks);
            }
            let life = play.life(enemy);
            if hit.is_none() && life < full {
                hit = Some(ticks);
            }
            if life.is_none_or(|life| life == Num::ZERO) {
                let hit = hit.ok_or(CheckFailure::Unsteered)?;
                let shot = shot
                    .filter(|&shot| shot <= hit)
                    .ok_or(CheckFailure::Unsteered)?;
                return Ok(MissileTicks {
                    built,
                    trained,
                    shot,
                    hit,
                    death: ticks,
                });
            }
        }
        Err(CheckFailure::NoMissileKill)
    }
}
