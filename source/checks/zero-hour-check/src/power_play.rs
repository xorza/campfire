use std::num::NonZeroU32;

use campfire_package::ModePackages;

use crate::failure::CheckFailure;
use crate::match_play::{AgainstAmerica, BuildingDozer, MatchPlay, PlannedBuilding, PlayTools};
use crate::pins::PowerSteps;

/// America's dozer.
const DOZER: &str = "americavehicledozer";

/// The Patriot battery, a defense of `KindOf = POWERED`.
const PATRIOT: &str = "americapatriotbattery";

/// The power play on Tournament Desert (power.md, A disabled unit is the rules package's): player 1,
/// America, builds a power plant, which gives 5; a Patriot battery, which takes 3, its
/// prerequisite the plant; a second, which takes 3 more, 6 of 5, so the player falls short and
/// both batteries stand disabled; and a second plant, 10 of 6, under which both stand again.
/// Each building takes its time at its player's power, the first plant at half speed, as a
/// player of no power and no use builds; whether the first battery stands disabled after each
/// building's tick and the next, as power counts as a tick ends and its modifier holds from the
/// next tick's grants.
#[derive(Debug)]
pub(crate) struct PowerPlay;

impl PowerPlay {
    /// The buildings, by player 1's start, clear of the cliffs, at America's
    /// `PlacementViewAngle`.
    const BUILDINGS: [PlannedBuilding; 4] = [
        PlannedBuilding {
            unit: "americapowerplant",
            action: "build_americapowerplant",
            at: [855, -584, -135],
        },
        PlannedBuilding {
            unit: PATRIOT,
            action: "build_americapatriotbattery",
            at: [1005, -800, -135],
        },
        PlannedBuilding {
            unit: PATRIOT,
            action: "build_americapatriotbattery",
            at: [1105, -800, -135],
        },
        PlannedBuilding {
            unit: "americapowerplant",
            action: "build_americapowerplant",
            at: [855, -704, -135],
        },
    ];

    /// The ticks each building took and whether the first battery stood disabled after each, in
    /// a match of `packages` at `rate` ticks a second; a failure for a step that does not end, a
    /// unit or an action the play lacks, or an option the mode does not take, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        tools: PlayTools,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<PowerSteps> {
        AgainstAmerica::AMERICA
            .play(packages, rate, tools, Self::steps)
            .map_err(|failure| failures.push(failure))
            .ok()
    }

    /// The steps in `play` of `packages`, after its start's tick.
    fn steps(
        play: &mut MatchPlay<'_>,
        packages: &ModePackages,
    ) -> Result<PowerSteps, CheckFailure> {
        play.step();
        let [dozer] = play.owned(DOZER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
        let builder = BuildingDozer {
            unit: DOZER,
            id: dozer,
        };
        let mut steps = PowerSteps {
            built: [0; 4],
            disabled: [false; 4],
        };
        for (at, &building) in Self::BUILDINGS.iter().enumerate() {
            let earlier = Self::BUILDINGS[..at].iter();
            let count = 1 + earlier
                .filter(|before| before.unit == building.unit)
                .count();
            steps.built[at] = play.build_of(packages, builder, building, count)?;
            play.step();
            let batteries = play.owned(PATRIOT, 0)?;
            steps.disabled[at] = batteries
                .first()
                .is_some_and(|&unit| play.blocks_attacks(unit));
        }
        Ok(steps)
    }
}
