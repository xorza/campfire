use std::num::NonZeroU32;

use campfire_harness::FixedSession;
use campfire_mode::{OptionPicks, OptionValue, PlayerSlotPick};
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, PlayerSlotPlan};
use campfire_script::MatchNames;
use campfire_values::ResourceId;

use crate::failure::CheckFailure;
use crate::match_play::{
    BuildingDozer, CHINA, CHINA_DOZER, MatchPlay, PlannedBuilding, START_MONEY, STEP_LIMIT,
};
use crate::pins::{DELIVERIES, SupplyDelivery, SupplySteps};

/// China's command center, and the supply center's free truck.
const CENTER: &str = "chinacommandcenter";
const TRUCK: &str = "chinavehiclesupplytruck";

/// The buildings the dozer builds, the power plant the supply center needs first, then the
/// center: the plant 150 m along −x from `Player_1_Start`, where the skirmish play's America
/// builds its own, and the center halfway from the start toward the supply dock at (632, −127),
/// 294 m from the dock's center, each clear of the cliffs, at China's `PlacementViewAngle` of
/// −45°.
const BUILDINGS: [PlannedBuilding; 2] = [
    PlannedBuilding {
        unit: "chinapowerplant",
        action: "build_chinapowerplant",
        at: [855, -584, -45],
    },
    PlannedBuilding {
        unit: "chinasupplycenter",
        action: "build_chinasupplycenter",
        at: [818, -355, -45],
    },
];

/// The supply on Tournament Desert, in a match of player 1 alone, as China (09-zero-hour.md,
/// A box is money): its dozer builds a power plant and a supply center, whose free truck gathers
/// at the nearest dock and delivers its boxes.
#[derive(Debug)]
pub(crate) struct SupplyPlay;

impl SupplyPlay {
    /// Plays the supply in a match of `packages` at `rate` ticks a second: the ticks of each
    /// building, and the tick and money of each of the truck's first `DELIVERIES` deliveries,
    /// from the center's completion; a failure for a step that does not end, a unit or an action
    /// the play lacks, or an option or a pick the mode does not take, goes to `failures`.
    pub(crate) fn run(
        packages: ModePackages,
        rate: NonZeroU32,
        failures: &mut Vec<CheckFailure>,
    ) -> Option<SupplySteps> {
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let china: PlayerSlotPick = CHINA.parse().expect("the play's pick");
        let session = FixedSession::planned(packages, InputRules::ROOMY, [PlayerSlotPlan::Player])
            .rate(rate)
            .picked(picks.places())
            .player_slot_picked(&[china]);
        let mut play = MatchPlay::new(session.start(), session.packages());
        let names = play.game().resource::<MatchNames>();
        let Some(money) = names.named::<ResourceId>("money") else {
            failures.push(CheckFailure::NoMoney);
            failures.extend(play.end().err());
            return None;
        };
        let played = Self::steps(&mut play, session.packages(), money);
        let ended = play.end();
        match played.and_then(|played| ended.map(|()| played)) {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
    }

    /// The steps in `play` of `packages`, after its start's tick, of player 1's `money`.
    fn steps(
        play: &mut MatchPlay,
        packages: &ModePackages,
        money: ResourceId,
    ) -> Result<SupplySteps, CheckFailure> {
        play.step();
        let [_center] = play.owned(CENTER, 0)?[..] else {
            return Err(CheckFailure::ProductionStart);
        };
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
        // The center frees its truck as it completes.
        let [_truck] = play.owned(TRUCK, 0)?[..] else {
            return Err(CheckFailure::NoTruck);
        };
        let built_money = play.money(money);
        let mut deliveries = [SupplyDelivery { ticks: 0, money: 0 }; DELIVERIES];
        let mut last = built_money;
        let mut delivered = 0;
        for ticks in 1..=STEP_LIMIT {
            play.step();
            let now = play.money(money);
            if now != last {
                deliveries[delivered] = SupplyDelivery { ticks, money: now };
                delivered += 1;
                last = now;
                if delivered == DELIVERIES {
                    return Ok(SupplySteps {
                        built,
                        built_money,
                        deliveries,
                    });
                }
            }
        }
        Err(CheckFailure::NoDelivery(delivered))
    }
}
