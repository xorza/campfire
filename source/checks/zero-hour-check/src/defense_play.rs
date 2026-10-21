use std::num::NonZeroU32;

use campfire_bots::{BotNames, BotScript, Location, OrderScript, ScriptedOrder, ScriptedUnits};
use campfire_capabilities::Directive;
use campfire_common::DeclaredName;
use campfire_harness::{FixedSession, MatchUnits};
use campfire_math::Num;
use campfire_mode::{OptionPicks, OptionValue, SeatPick};
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, SeatPlan};
use campfire_units::MapPoint;

use crate::failure::{CheckFailure, Differs};
use crate::match_play::{BuildingDozer, DOZER, MatchPlay, PlannedBuilding, STEP_LIMIT, america};
use crate::pins::DefenseTicks;

/// The money player 1 starts with, as the skirmish play's host picks it.
const START_MONEY: &str = "start_money=50000";

/// Player 1's pick of China.
const CHINA: &str = "0:faction=china";

/// China's dozer.
const CHINA_DOZER: &str = "chinavehicledozer";

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
        let start_money: OptionValue = START_MONEY.parse().expect("the play's option");
        let picks = match OptionPicks::named(&packages.data().options, &[start_money]) {
            Ok(picks) => picks,
            Err(error) => {
                failures.push(CheckFailure::Options(error));
                return None;
            }
        };
        let plan = [SeatPlan::Player, SeatPlan::Bot];
        let china: SeatPick = CHINA.parse().expect("the play's pick");
        let bot = america(2).pop().expect("the bot's pick of America");
        let session = FixedSession::planned(packages, InputRules::ROOMY, plan)
            .rate(rate)
            .picked(picks.places())
            .seat_picked(&[china, bot]);
        let mut play = MatchPlay::new(session.start());
        let played = Self::steps(&mut play, session.packages());
        play.end();
        match played {
            Ok(played) => Some(played),
            Err(failure) => {
                failures.push(failure);
                None
            }
        }
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
        let at = MatchUnits::of(play.game()).position(cannon);
        let to = MapPoint::Ground([at.x, at.z - SHORT_OF_THE_CANNON]);
        let drive = ScriptedOrder {
            tick: play.tick(),
            units: ScriptedUnits::Typed(DeclaredName::new(DOZER).expect("the dozer's name")),
            directive: Directive::Orders(OrdersDirective::Move {
                to: Location::At(to),
            }),
        };
        let script = OrderScript::of_orders([drive]).map_err(CheckFailure::BotOrders)?;
        let books = packages.name_books();
        let bot =
            BotScript::new(&script, &BotNames::new(&books)).map_err(CheckFailure::BotNames)?;
        play.drive_bot(1, bot)?;
        let [enemy] = play.owned(DOZER, 1)?[..] else {
            return Err(CheckFailure::FightStart);
        };
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
