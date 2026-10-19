use campfire_actions::ActionTarget;
use campfire_capabilities::Order;
use campfire_math::{Num, Vec3};
use campfire_orders::OrdersAction;
use campfire_production::TrainQueue;
use campfire_protocol::{InputRules, SeatPlan};
use campfire_sim::StableId;
use campfire_test_packages::TestMode;
use campfire_units::GroundPoint;
use campfire_values::Team;

use crate::fixed_session::FixedSession;
use crate::fixed_session::fixed_match::FixedMatch;
use crate::match_units::MatchUnits;
use crate::scripted::{Aim, Plan, Scripted, TICK_HZ};
use crate::scripted_mode::ScriptedMode;

/// The camps team, which holds the neutral boulder.
const CAMPS: Team = Team::new(2);

/// The proving mode of `packages/test`: a small match that uses every capability the release
/// runs, played by two scripted players, the second of whom joins slot 1, open at the start,
/// before tick `JOIN`, and leaves it, reserved, before tick `LEAVE`. It is owned by the tests, so
/// no balance change of the MOBA test content moves it. Its plan is what a proving player orders,
/// by the role of the units it names.
#[derive(Debug, Clone, Copy)]
pub enum ProvingMatch {
    /// The hero walks to a point.
    Move { x: i64, z: i64 },
    /// The hero casts the action in `slot` at `at`.
    Cast { slot: u8, at: Aim },
    /// The hero attacks the boulder.
    AttackBoulder,
    /// The hero attacks the hero of player `seat`.
    AttackHero { seat: u32 },
    /// The player's barracks of that place, by stable id, trains a guard.
    Train { barracks: usize },
    /// The player's barracks of that place posts a ward at a point.
    Ward { barracks: usize, x: i64, z: i64 },
}

/// The players' inputs, in the order of their stamps. Player 0 plays the lancer for north, player
/// 1 the sage for south.
///
/// - Stamp 2: both of north's barracks order a guard with gold for one, and south's barracks
///   orders one: the lower id of north's two trains.
/// - The lancer attacks the sage in tick 5, while north does not see her, which does nothing,
///   and again in tick 29, once north sees her, until the lancer's cast of tick 30.
/// - The heroes walk to the middle and trade their abilities: the lancer's fan, quake and snare,
///   the sage's orb and nova. The sage then posts a wisp, a unit type of its own package.
/// - North's barracks compete again once the income of tick 100 comes.
/// - The lancer walks off the boulder's long side, then breaks it: a box turned 30°, a static
///   body near the lane, which its weapon reaches from the box's edge.
/// - North's east barracks posts a ward by the south's barracks, which no unit of north sees
///   otherwise.
const SCRIPT: [Scripted<ProvingMatch>; 24] = [
    Scripted::join(ProvingMatch::JOIN, 1),
    Scripted::leave(ProvingMatch::LEAVE, 1),
    Scripted::new(2, 0, ProvingMatch::Train { barracks: 0 }),
    Scripted::new(2, 0, ProvingMatch::Train { barracks: 1 }),
    Scripted::new(2, 1, ProvingMatch::Train { barracks: 0 }),
    Scripted::new(3, 0, ProvingMatch::Move { x: -1, z: -1 }),
    Scripted::new(3, 1, ProvingMatch::Move { x: 2, z: 2 }),
    Scripted::new(5, 0, ProvingMatch::AttackHero { seat: 1 }),
    Scripted::new(29, 0, ProvingMatch::AttackHero { seat: 1 }),
    cast(30, 0, 0, Aim::HeroPoint { slot: 1 }),
    cast(40, 1, 0, Aim::Hero { slot: 0 }),
    cast(60, 0, 1, Aim::HeroPoint { slot: 1 }),
    cast(70, 1, 1, Aim::Nothing),
    cast(80, 0, 2, Aim::Nothing),
    cast(100, 1, 2, Aim::Point { x: 2, z: 6 }),
    Scripted::new(105, 0, ProvingMatch::Train { barracks: 1 }),
    Scripted::new(105, 0, ProvingMatch::Train { barracks: 0 }),
    Scripted::new(120, 0, ProvingMatch::Move { x: -3, z: 4 }),
    Scripted::new(150, 0, ProvingMatch::AttackBoulder),
    Scripted::new(160, 1, ProvingMatch::Move { x: -6, z: -1 }),
    cast(200, 0, 0, Aim::HeroPoint { slot: 1 }),
    cast(260, 1, 0, Aim::Hero { slot: 0 }),
    cast(260, 0, 1, Aim::Point { x: -4, z: -1 }),
    Scripted::new(
        300,
        0,
        ProvingMatch::Ward {
            barracks: 1,
            x: 12,
            z: 9,
        },
    ),
];

/// A cast by the player `seat`'s hero of its ability slot `action` at `aim`.
const fn cast(stamp: u64, seat: u32, action: u8, aim: Aim) -> Scripted<ProvingMatch> {
    Scripted::new(
        stamp,
        seat,
        ProvingMatch::Cast {
            slot: action,
            at: aim,
        },
    )
}

impl ProvingMatch {
    pub const PLAYERS: u32 = 2;
    /// The ticks a test plays: 30 s at 20 a second.
    pub const TICKS: u64 = 600;
    /// The tick player 1 joins in, before their first order, and the one they leave in, after
    /// their last.
    pub const JOIN: u64 = 1;
    pub const LEAVE: u64 = 400;

    pub fn load() -> ScriptedMode<Self> {
        let packages = TestMode::Proving.packages();
        let plan = [SeatPlan::Player, SeatPlan::Open];
        let session = FixedSession::planned(packages, TICK_HZ, InputRules::ROOMY, plan);
        ScriptedMode::new(session, SCRIPT.to_vec())
    }
}

impl Plan for ProvingMatch {
    fn start(_: &mut FixedMatch, _: &[SeatPlan]) {}

    fn order(self, units: MatchUnits<'_>, seat: u32) -> Order {
        let hero = units.hero(seat);
        let (unit, action) = match self {
            Self::Move { x, z } => (
                hero,
                OrdersAction::Move {
                    to: GroundPoint {
                        x: Num::int(x),
                        z: Num::int(z),
                    },
                },
            ),
            Self::AttackBoulder => (
                hero,
                OrdersAction::Attack {
                    target: boulder(units),
                },
            ),
            Self::AttackHero { seat } => (
                hero,
                OrdersAction::Attack {
                    target: units.hero(seat),
                },
            ),
            Self::Train { barracks: at } => (
                barracks(units, seat)[at],
                OrdersAction::Slot {
                    slot: 0,
                    target: ActionTarget::None,
                },
            ),
            Self::Ward { barracks: at, x, z } => (
                barracks(units, seat)[at],
                OrdersAction::Slot {
                    slot: 1,
                    target: ActionTarget::Point(Vec3::from((Num::int(x), 0, Num::int(z)))),
                },
            ),
            Self::Cast { slot, at } => (
                hero,
                OrdersAction::Slot {
                    slot,
                    target: at.target(units),
                },
            ),
        };
        Order::one(unit, action)
    }
}

/// Player `seat`'s barracks, by stable id.
fn barracks(units: MatchUnits<'_>, seat: u32) -> Vec<StableId> {
    units
        .all()
        .filter(|(_, unit)| MatchUnits::owned_by(unit, seat) && unit.contains::<TrainQueue>())
        .map(|(id, _)| id)
        .collect()
}

/// The neutral boulder.
fn boulder(units: MatchUnits<'_>) -> StableId {
    units
        .all()
        .find(|(_, unit)| unit.get::<Team>() == Some(&CAMPS))
        .map(|(id, _)| id)
        .expect("the boulder stands until its order")
}
