use std::num::NonZeroU32;

use campfire_math::Num;
use campfire_package::ModePackages;

use crate::failure::{CheckFailure, Differs};
use crate::match_play::{self, BuildingDozer, CHINA_DOZER, MatchPlay, PlannedBuilding, STEP_LIMIT};
use crate::pins::DefenseTicks;

/// The power plant, which the cannon needs, where the supply play places its own, and the
/// cannon, 216 m along −z from `Player_1_Start`, where the skirmish play places its barracks,
/// clear of the cliffs, each at China's `PlacementViewAngle` of −45°.
const BUILDINGS: [PlannedBuilding; 2] = [
    PlannedBuilding {
        unit: "chinapowerplant",
        action: "build_chinapowerplant",
        at: [855, -584, -45],
    },
    PlannedBuilding {
        unit: CANNON,
        action: "build_chinagattlingcannon",
        at: [1005, -800, -45],
    },
];

/// The Gattling Cannon.
const CANNON: &str = "chinagattlingcannon";

/// How far south of the cannon the bot's dozer drives, in meters: within the 225 m its gun
/// reaches.
const SHORT_OF_THE_CANNON: Num = Num::int(150);

/// The ticks between the cannon's first eight hits, as the original's `FiringTracker` gives
/// them for `GattlingBuildingGun`: 250 ms are 8 frames; the second hit passes 1 shot, so the
/// third comes 8 ÷ 2 = 4 frames after it, and the sixth passes 5, so the seventh comes
/// ⌊8 ÷ 3⌋ = 2 after the one before it; each delay is read as its shot fires, before it counts.
const SPIN_UP: [u64; 7] = [8, 8, 4, 4, 4, 4, 2];

/// A base defense's defense on Tournament Desert (04-capabilities/combat.md, A shot streak
/// gives a stage's modifier; 04-capabilities/control.md, An idle unit scans for a target, as its
/// type declares): player 1, China, builds a power plant and a Gattling Cannon, and the bot,
/// America, drives its dozer from its start to a point within the cannon's reach, where the
/// cannon, ordered nothing, fires at it ever sooner, 1 damage a hit against `DozerArmor`, and
/// kills it.
#[derive(Debug)]
pub(crate) struct DefensePlay;

impl DefensePlay {
    /// Plays the defense in a match of `packages` at `rate` ticks a second: the power plant's
    /// ticks and the cannon's, and the ticks from the dozer's order to its first damage and to
    /// its death; a failure for a step that does not end, a unit or an action the play lacks, an
    /// option the mode does not take, or hits that do not come as the original's, goes to
    /// `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<DefenseTicks> {
        match_play::china_against_america(packages, rate, Self::steps)
            .map_err(|failure| failures.push(failure))
            .ok()
    }

    /// The steps in `play` of `packages`, after its start's tick.
    fn steps(play: &mut MatchPlay, packages: &ModePackages) -> Result<DefenseTicks, CheckFailure> {
        play.step();
        let [dozer] = play.owned(CHINA_DOZER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let builder = BuildingDozer {
            unit: CHINA_DOZER,
            id: dozer,
        };
        let mut built = [0; BUILDINGS.len()];
        for (&building, ticks) in BUILDINGS.iter().zip(&mut built) {
            *ticks = play.build(packages, builder, building)?;
        }
        let [cannon] = play.owned(CANNON, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let enemy = play.drive_bot_dozer(packages, cannon, SHORT_OF_THE_CANNON)?;
        let mut life = play.life(enemy);
        let mut hits = Vec::with_capacity(SPIN_UP.len() + 1);
        for ticks in 1..=STEP_LIMIT {
            play.step();
            let now = play.life(enemy);
            if now < life {
                hits.push(ticks);
                if hits.len() == SPIN_UP.len() + 1 {
                    let gaps = core::array::from_fn(|at| hits[at + 1] - hits[at]);
                    if let Some(differs) = Differs::of(gaps, SPIN_UP) {
                        return Err(CheckFailure::SpinUp(differs));
                    }
                }
            }
            life = now;
            if life.is_none_or(|life| life == Num::ZERO) {
                return Ok(DefenseTicks {
                    built,
                    hit: hits.first().copied().ok_or(CheckFailure::Undefended)?,
                    death: ticks,
                });
            }
        }
        Err(CheckFailure::Undefended)
    }
}
