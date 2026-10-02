use std::num::NonZeroU32;
use std::path::Path;

use bevy_ecs::world::EntityRef;
use campfire_capabilities::{Action, ActionTarget, Experience, Order, Owner, Team, TrainQueue};
use campfire_math::{Num, Tick, Vec3};
use campfire_package::ModePackages;
use campfire_sim::{EntityIndex, Position, StableId};

use crate::fixed_match::FixedMatch;
use crate::fixed_session::FixedSession;

/// The proving mode of `packages/test`: a small match that uses every capability the release
/// runs, played by two scripted players. It is owned by the tests, so no balance change of the
/// reference content moves it.
#[derive(Debug)]
pub struct ProvingMatch {
    session: FixedSession,
}

const MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/proving"
);
const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
/// The camps team, which holds the neutral boulder.
const CAMPS: Team = Team::new(2);

/// One scripted input: what player `slot` orders, sent for `stamp`.
#[derive(Debug, Clone, Copy)]
struct Scripted {
    stamp: u64,
    slot: u32,
    plan: Plan,
}

/// What a scripted input orders, by the role of the units it names, which the match resolves to
/// stable ids when it sends the input.
#[derive(Debug, Clone, Copy)]
enum Plan {
    /// The hero walks to a point.
    Move { x: i64, z: i64 },
    /// The hero casts the action in `slot` at `at`.
    Cast { slot: u8, at: Aim },
    /// The hero attacks the boulder.
    AttackBoulder,
    /// The player's barracks of that place, by stable id, trains a guard.
    Train { barracks: usize },
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
const SCRIPT: [Scripted; 17] = [
    order(2, 0, Plan::Train { barracks: 0 }),
    order(2, 0, Plan::Train { barracks: 1 }),
    order(2, 1, Plan::Train { barracks: 0 }),
    order(3, 0, Plan::Move { x: -1, z: -1 }),
    order(3, 1, Plan::Move { x: 2, z: 2 }),
    order(
        30,
        0,
        Plan::Cast {
            slot: 0,
            at: Aim::EnemyHeroPoint,
        },
    ),
    order(
        40,
        1,
        Plan::Cast {
            slot: 0,
            at: Aim::EnemyHero,
        },
    ),
    order(
        60,
        0,
        Plan::Cast {
            slot: 1,
            at: Aim::EnemyHeroPoint,
        },
    ),
    order(
        70,
        1,
        Plan::Cast {
            slot: 1,
            at: Aim::Nothing,
        },
    ),
    order(
        80,
        0,
        Plan::Cast {
            slot: 2,
            at: Aim::Nothing,
        },
    ),
    order(105, 0, Plan::Train { barracks: 1 }),
    order(105, 0, Plan::Train { barracks: 0 }),
    order(120, 0, Plan::AttackBoulder),
    order(160, 1, Plan::Move { x: -6, z: -1 }),
    order(
        200,
        0,
        Plan::Cast {
            slot: 0,
            at: Aim::EnemyHeroPoint,
        },
    ),
    order(
        260,
        1,
        Plan::Cast {
            slot: 0,
            at: Aim::EnemyHero,
        },
    ),
    order(
        260,
        0,
        Plan::Cast {
            slot: 1,
            at: Aim::Point { x: -4, z: -1 },
        },
    ),
];

const fn order(stamp: u64, slot: u32, plan: Plan) -> Scripted {
    Scripted { stamp, slot, plan }
}

impl ProvingMatch {
    pub const PLAYERS: u32 = 2;
    /// The ticks a test plays: 30 s at 20 a second.
    pub const TICKS: u64 = 600;

    pub fn load() -> ProvingMatch {
        let packages =
            ModePackages::from_dir(Path::new(MODE)).unwrap_or_else(|error| panic!("{error}"));
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
        for scripted in SCRIPT.iter().filter(|scripted| scripted.stamp == tick) {
            let order = ProvingMatch::resolve(fixed, scripted);
            fixed.send(scripted.slot, Tick::new(tick), &Order::payload(&[order]));
        }
        fixed.runner_mut().run_tick();
    }

    /// The order `scripted` stands for, with its units' stable ids now.
    fn resolve(fixed: &FixedMatch, scripted: &Scripted) -> Order {
        let units = Units::of(fixed);
        let hero = units.hero(scripted.slot);
        let enemy = units.hero(1 - scripted.slot);
        let (unit, action) = match scripted.plan {
            Plan::Move { x, z } => (
                hero,
                Action::Move {
                    x: num(x),
                    z: num(z),
                },
            ),
            Plan::AttackBoulder => (
                hero,
                Action::Attack {
                    target: units.boulder(),
                },
            ),
            Plan::Train { barracks } => (
                units.barracks(scripted.slot)[barracks],
                Action::Slot {
                    slot: 0,
                    target: ActionTarget::None,
                },
            ),
            Plan::Cast { slot, at } => {
                let target = match at {
                    Aim::Nothing => ActionTarget::None,
                    Aim::Point { x, z } => ActionTarget::Point(ground(num(x), num(z))),
                    Aim::EnemyHeroPoint => ActionTarget::Point(units.position(enemy)),
                    Aim::EnemyHero => ActionTarget::Unit(enemy),
                };
                (hero, Action::Slot { slot, target })
            }
        };
        Order { unit, action }
    }
}

/// The units of a running match that the script names.
#[derive(Debug)]
struct Units<'a> {
    fixed: &'a FixedMatch,
}

impl Units<'_> {
    const fn of(fixed: &FixedMatch) -> Units<'_> {
        Units { fixed }
    }

    /// Every unit, with its stable id, in the order of the ids.
    fn all(&self) -> impl Iterator<Item = (StableId, EntityRef<'_>)> {
        let world = self.fixed.runner().world();
        world
            .resource::<EntityIndex>()
            .iter()
            .map(move |(id, entity)| (id, world.entity(entity)))
    }

    /// Player `slot`'s hero: the unit it owns that gains experience.
    fn hero(&self, slot: u32) -> StableId {
        self.all()
            .find(|(_, unit)| owned_by(unit, slot) && unit.contains::<Experience>())
            .map(|(id, _)| id)
            .expect("each player has a hero")
    }

    /// Player `slot`'s barracks, by stable id.
    fn barracks(&self, slot: u32) -> Vec<StableId> {
        self.all()
            .filter(|(_, unit)| owned_by(unit, slot) && unit.contains::<TrainQueue>())
            .map(|(id, _)| id)
            .collect()
    }

    fn boulder(&self) -> StableId {
        self.all()
            .find(|(_, unit)| unit.get::<Team>() == Some(&CAMPS))
            .map(|(id, _)| id)
            .expect("the boulder stands until its order")
    }

    fn position(&self, id: StableId) -> Position {
        let world = self.fixed.runner().world();
        let entity = world
            .resource::<EntityIndex>()
            .get(id)
            .expect("a unit of the match");
        *world
            .get::<Position>(entity)
            .expect("a unit stands somewhere")
    }
}

fn owned_by(unit: &EntityRef<'_>, slot: u32) -> bool {
    unit.get::<Owner>()
        .is_some_and(|owner| owner.slot().get() == slot)
}

const fn num(value: i64) -> Num {
    Num::from_int(value).expect("a small integer")
}

const fn ground(x: Num, z: Num) -> Position {
    Position::new(Vec3::new(x, Num::ZERO, z)).expect("a point of the map")
}
