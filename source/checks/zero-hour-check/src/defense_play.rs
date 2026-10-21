use std::num::NonZeroU32;

use campfire_math::Num;
use campfire_package::ModePackages;

use crate::failure::{CheckFailure, Differs};
use crate::match_play::{AgainstAmerica, BuildingDozer, MatchPlay, PlannedBuilding, STEP_LIMIT};
use crate::pins::DefenseTicks;

/// How far south of its defense the bot's dozer drives, in meters: within the 225 m that the
/// Gattling Cannon's gun, the Patriot battery's missiles and the Stinger soldiers' reach.
const SHORT_OF_THE_DEFENSE: Num = Num::int(150);

/// A base defense's defense on Tournament Desert (04-capabilities/combat.md, A shot streak
/// gives a stage's modifier; 04-capabilities/control.md, An idle unit scans for a target, as its
/// type declares; 04-capabilities/production.md, A building may stand on a circle): player 1
/// builds a power plant, where the supply play places its own, and the defense, 216 m along −z
/// from `Player_1_Start`, where the skirmish play places its barracks, clear of the cliffs, at its
/// faction's `PlacementViewAngle`; and the bot, America, drives its dozer from its start to a
/// point within the defense's reach, where the defense, ordered nothing, fires at it and kills
/// it. Its hits come apart as `spin_up` says, when it names their gaps.
#[derive(Debug)]
pub(crate) struct DefensePlay {
    against: AgainstAmerica,
    dozer: &'static str,
    buildings: [PlannedBuilding; 2],
    spin_up: Option<[u64; 7]>,
}

impl DefensePlay {
    /// China's Gattling Cannon, whose first eight hits come as the original's `FiringTracker`
    /// gives them for `GattlingBuildingGun`, 1 damage a hit against `DozerArmor`: 250 ms are 8
    /// frames; the second hit passes 1 shot, so the third comes 8 ÷ 2 = 4 frames after it, and the
    /// sixth passes 5, so the seventh comes ⌊8 ÷ 3⌋ = 2 after the one before it; each delay is
    /// read as its shot fires, before it counts.
    pub(crate) const GATTLING: Self = Self {
        against: AgainstAmerica::CHINA,
        dozer: "chinavehicledozer",
        buildings: [
            PlannedBuilding {
                unit: "chinapowerplant",
                action: "build_chinapowerplant",
                at: [855, -584, -45],
            },
            PlannedBuilding {
                unit: "chinagattlingcannon",
                action: "build_chinagattlingcannon",
                at: [1005, -800, -45],
            },
        ],
        spin_up: Some([8, 8, 4, 4, 4, 4, 2]),
    };

    /// America's Patriot battery, a building on a circle, whose steered missiles kill the dozer.
    pub(crate) const PATRIOT: Self = Self {
        against: AgainstAmerica::AMERICA,
        dozer: "americavehicledozer",
        buildings: [
            PlannedBuilding {
                unit: "americapowerplant",
                action: "build_americapowerplant",
                at: [855, -584, -135],
            },
            PlannedBuilding {
                unit: "americapatriotbattery",
                action: "build_americapatriotbattery",
                at: [1005, -800, -135],
            },
        ],
        spin_up: None,
    };

    /// GLA's Stinger site, whose three kept Stinger soldiers kill the dozer with their missiles
    /// (04-capabilities/production.md, A building may keep spawns).
    pub(crate) const STINGER: Self = Self {
        against: AgainstAmerica::GLA,
        dozer: "glainfantryworker",
        buildings: [
            PlannedBuilding {
                unit: "glabarracks",
                action: "build_glabarracks",
                at: [855, -584, -45],
            },
            PlannedBuilding {
                unit: "glastingersite",
                action: "build_glastingersite",
                at: [1005, -800, -45],
            },
        ],
        spin_up: None,
    };

    /// Plays the defense in a match of `packages` at `rate` ticks a second: the power plant's
    /// ticks and the defense's, and the ticks from the dozer's order to its first damage and to
    /// its death; a failure for a step that does not end, a unit or an action the play lacks, an
    /// option the mode does not take, or hits that do not come as the original's, goes to
    /// `failures`.
    pub(crate) fn run(
        &self,
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<DefenseTicks> {
        self.against
            .play(packages, rate, |play, packages| self.steps(play, packages))
            .map_err(|failure| failures.push(failure))
            .ok()
    }

    /// The steps in `play` of `packages`, after its start's tick.
    fn steps(
        &self,
        play: &mut MatchPlay,
        packages: &ModePackages,
    ) -> Result<DefenseTicks, CheckFailure> {
        play.step();
        let [dozer] = play.owned(self.dozer, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let builder = BuildingDozer {
            unit: self.dozer,
            id: dozer,
        };
        let mut built = [0; 2];
        for (&building, ticks) in self.buildings.iter().zip(&mut built) {
            *ticks = play.build(packages, builder, building)?;
        }
        let [defense] = play.owned(self.buildings[1].unit, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let enemy = play.drive_bot_dozer(packages, defense, SHORT_OF_THE_DEFENSE)?;
        let mut life = play.life(enemy);
        let mut hits = Vec::with_capacity(8);
        for ticks in 1..=STEP_LIMIT {
            play.step();
            let now = play.life(enemy);
            if now < life {
                hits.push(ticks);
                if let Some(spin_up) = self.spin_up
                    && hits.len() == spin_up.len() + 1
                {
                    let gaps = core::array::from_fn(|at| hits[at + 1] - hits[at]);
                    if let Some(differs) = Differs::of(gaps, spin_up) {
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
