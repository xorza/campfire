use std::num::NonZeroU32;

use campfire_capabilities::{Action, ActionTarget, Order, Team, TrainQueue};
use campfire_math::{Num, Vec3};
use campfire_package::{ModePackages, PackageDir};
use campfire_sim::{Position, StableId};

use crate::fixed_match::FixedMatch;
use crate::fixed_session::FixedSession;
use crate::match_units::MatchUnits;
use crate::scripted::{Plan, Scripted};

/// The proving mode of `packages/test`: a small match that uses every capability the release
/// runs, played by two scripted players. It is owned by the tests, so no balance change of the
/// reference content moves it.
#[derive(Debug)]
pub struct ProvingMatch {
    session: FixedSession,
}

const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
/// The camps team, which holds the neutral boulder.
const CAMPS: Team = Team::new(2);

/// What a proving player orders, by the role of the units it names.
#[derive(Debug, Clone, Copy)]
enum ProvingPlan {
    /// The hero walks to a point.
    Move { x: i64, z: i64 },
    /// The hero casts the action in `slot` at `at`.
    Cast { slot: u8, at: Aim },
    /// The hero attacks the boulder.
    AttackBoulder,
    /// The player's barracks of that place, by stable id, trains a guard.
    Train { barracks: usize },
    /// The player's barracks of that place posts a ward at a point.
    Ward { barracks: usize, x: i64, z: i64 },
}

/// Where a scripted cast aims.
#[derive(Debug, Clone, Copy)]
enum Aim {
    Nothing,
    Point {
        x: i64,
        z: i64,
    },
    /// Where the enemy hero stands when the input is sent.
    EnemyHeroPoint,
    /// The enemy hero itself.
    EnemyHero,
}

/// The players' inputs, in the order of their stamps. Player 0 plays the lancer for north, player
/// 1 the sage for south.
///
/// - Stamp 2: both of north's barracks order a guard with gold for one, and south's barracks
///   orders one: the lower id of north's two trains.
/// - The heroes walk to the middle and trade their abilities: the lancer's fan, quake and snare,
///   the sage's orb and nova.
/// - North's barracks compete again once the income of tick 100 comes.
/// - The lancer breaks the boulder, a static body near the lane.
/// - North's east barracks posts a ward by the south's barracks, which no unit of north sees
///   otherwise.
const SCRIPT: [Scripted<ProvingPlan>; 18] = [
    order(2, 0, ProvingPlan::Train { barracks: 0 }),
    order(2, 0, ProvingPlan::Train { barracks: 1 }),
    order(2, 1, ProvingPlan::Train { barracks: 0 }),
    order(3, 0, ProvingPlan::Move { x: -1, z: -1 }),
    order(3, 1, ProvingPlan::Move { x: 2, z: 2 }),
    cast(30, 0, 0, Aim::EnemyHeroPoint),
    cast(40, 1, 0, Aim::EnemyHero),
    cast(60, 0, 1, Aim::EnemyHeroPoint),
    cast(70, 1, 1, Aim::Nothing),
    cast(80, 0, 2, Aim::Nothing),
    order(105, 0, ProvingPlan::Train { barracks: 1 }),
    order(105, 0, ProvingPlan::Train { barracks: 0 }),
    order(120, 0, ProvingPlan::AttackBoulder),
    order(160, 1, ProvingPlan::Move { x: -6, z: -1 }),
    cast(200, 0, 0, Aim::EnemyHeroPoint),
    cast(260, 1, 0, Aim::EnemyHero),
    cast(260, 0, 1, Aim::Point { x: -4, z: -1 }),
    order(
        300,
        0,
        ProvingPlan::Ward {
            barracks: 1,
            x: 12,
            z: 9,
        },
    ),
];

const fn order(stamp: u64, slot: u32, plan: ProvingPlan) -> Scripted<ProvingPlan> {
    Scripted::new(stamp, slot, plan)
}

/// A cast by the player `slot`'s hero of its ability slot `ability` at `at`.
const fn cast(stamp: u64, slot: u32, ability: u8, at: Aim) -> Scripted<ProvingPlan> {
    order(stamp, slot, ProvingPlan::Cast { slot: ability, at })
}

impl ProvingMatch {
    pub const PLAYERS: u32 = 2;
    /// The ticks a test plays: 30 s at 20 a second.
    pub const TICKS: u64 = 600;

    pub fn load() -> ProvingMatch {
        let packages = ModePackages::from_dir(&PackageDir::workspace("test/modes/proving"))
            .unwrap_or_else(|error| panic!("{error}"));
        ProvingMatch {
            session: FixedSession::new(packages, TICK_HZ, ProvingMatch::PLAYERS),
        }
    }

    pub const fn packages(&self) -> &ModePackages {
        self.session.packages()
    }

    /// A match at tick 0, no input sent yet.
    pub fn start(&self) -> FixedMatch {
        self.session.start()
    }

    /// Runs tick `tick` of `fixed`: first sends the scripted inputs stamped for it, then runs it.
    pub fn play_tick(fixed: &mut FixedMatch, tick: u64) {
        Scripted::play_tick(&SCRIPT, fixed, tick);
    }
}

impl Plan for ProvingPlan {
    fn order(self, units: MatchUnits<'_>, slot: u32) -> Order {
        let hero = units.hero(slot);
        let enemy = units.hero(1 - slot);
        let (unit, action) = match self {
            ProvingPlan::Move { x, z } => (
                hero,
                Action::Move {
                    x: Num::int(x),
                    z: Num::int(z),
                },
            ),
            ProvingPlan::AttackBoulder => (
                hero,
                Action::Attack {
                    target: boulder(units),
                },
            ),
            ProvingPlan::Train { barracks: at } => (
                barracks(units, slot)[at],
                Action::Slot {
                    slot: 0,
                    target: ActionTarget::None,
                },
            ),
            ProvingPlan::Ward { barracks: at, x, z } => (
                barracks(units, slot)[at],
                Action::Slot {
                    slot: 1,
                    target: ActionTarget::Point(ground(Num::int(x), Num::int(z))),
                },
            ),
            ProvingPlan::Cast { slot, at } => {
                let target = match at {
                    Aim::Nothing => ActionTarget::None,
                    Aim::Point { x, z } => ActionTarget::Point(ground(Num::int(x), Num::int(z))),
                    Aim::EnemyHeroPoint => ActionTarget::Point(units.position(enemy)),
                    Aim::EnemyHero => ActionTarget::Unit(enemy),
                };
                (hero, Action::Slot { slot, target })
            }
        };
        Order { unit, action }
    }
}

/// Player `slot`'s barracks, by stable id.
fn barracks(units: MatchUnits<'_>, slot: u32) -> Vec<StableId> {
    units
        .all()
        .filter(|(_, unit)| MatchUnits::owned_by(unit, slot) && unit.contains::<TrainQueue>())
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

const fn ground(x: Num, z: Num) -> Position {
    Position::new(Vec3::new(x, Num::ZERO, z)).expect("a point of the map")
}
