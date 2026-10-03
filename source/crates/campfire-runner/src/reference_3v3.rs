use std::num::NonZeroU32;

use campfire_capabilities::{
    Action, ActionTarget, Combat, Dead, InputValue, ModeInput, Order, PathWalker, Pools, Team,
};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::ServerSeed;
use campfire_sim::{Position, StableId};

use crate::fixed_match::FixedMatch;
use crate::fixed_session::FixedSession;
use crate::match_units::MatchUnits;
use crate::scripted::{Plan, Scripted};

/// The reference 3v3 as its packages hold it, at its slowest rate, which plays a match in the
/// fewest ticks, with six players of fixed keys.
#[derive(Debug)]
pub struct Reference3v3 {
    session: FixedSession,
}

const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
/// The hero each slot picks.
const HEROES: [&str; 6] = [
    "hero-cinder",
    "hero-gale",
    "hero-husk",
    "hero-kensho",
    "hero-rime",
    "hero-veil",
];

/// A play of the reference 3v3: the orders its scripted players send after the pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    /// The north's heroes fight the west camp's wolf.
    Camp,
    /// Before the first wave, two north heroes fell Rime, and one heals them; Husk, hastened,
    /// strikes Kensho beside the south's west outer tower and falls to it.
    Skirmish,
    /// Cinder walks to the west lane and Veil to the east one, and each strikes the enemy creep
    /// of least life near it while the first waves fight.
    Lanes,
}

/// What a 3v3 player orders, by the role of the units it names.
#[derive(Debug, Clone, Copy)]
enum ReferencePlan {
    /// The hero walks to a point.
    Move { x: i64, z: i64 },
    /// The hero attacks the unit that spawned at a point: a camp's, or a structure.
    AttackSpawnedAt { x: i64, z: i64 },
    /// The hero attacks player `slot`'s hero.
    AttackHero { slot: u32 },
    /// The hero casts its spell in `slot`, which aims at nothing.
    Cast { slot: u8 },
    /// The hero attacks the enemy creep of least life within `FARM_REACH` of it, by stable id on
    /// a tie; with none, it stands where it is.
    Farm,
}

/// How far from a hero a creep it farms stands at most, in meters.
const FARM_REACH: i64 = 10;

/// A hero's slot of the spell `haste` and of `mend`, as each player picks them: after its three
/// basic abilities and its ultimate.
const HASTE: u8 = 4;
const MEND: u8 = 5;

/// The west camp's wolf, on its marker.
const WEST_CAMP: (i64, i64) = (-18, -12);

/// The camp: Husk, player 2's hero, walks from the north spawn to the west camp's wolf and
/// strikes it; it walks back south past the wolf's leash, which sends the wolf home; then Husk,
/// Cinder and Gale, players 0 and 1's heroes, who waited south of the camp, fell it.
const CAMP: [Scripted<ReferencePlan>; 7] = [
    order(1200, 2, attack_at(WEST_CAMP)),
    order(1200, 0, ReferencePlan::Move { x: -20, z: -24 }),
    order(1200, 1, ReferencePlan::Move { x: -16, z: -24 }),
    order(1560, 2, ReferencePlan::Move { x: -18, z: -32 }),
    order(1700, 0, attack_at(WEST_CAMP)),
    order(1700, 1, attack_at(WEST_CAMP)),
    order(1700, 2, attack_at(WEST_CAMP)),
];

/// The skirmish: players 0 to 2 hold Cinder, Gale and Husk, north; 3 to 5 Kensho, Rime and
/// Veil, south.
const SKIRMISH: [Scripted<ReferencePlan>; 13] = [
    order(1200, 0, ReferencePlan::Move { x: -2, z: -26 }),
    order(1200, 1, ReferencePlan::Move { x: 2, z: -26 }),
    order(1200, 4, ReferencePlan::Move { x: 0, z: -20 }),
    order(1200, 3, ReferencePlan::Move { x: -38, z: 12 }),
    order(1200, 2, ReferencePlan::Cast { slot: HASTE }),
    order(1201, 2, ReferencePlan::Move { x: -34, z: 4 }),
    order(1700, 0, ReferencePlan::AttackHero { slot: 4 }),
    order(1700, 1, ReferencePlan::AttackHero { slot: 4 }),
    order(1700, 4, ReferencePlan::AttackHero { slot: 1 }),
    order(1880, 1, ReferencePlan::Cast { slot: MEND }),
    order(1880, 0, ReferencePlan::Move { x: -36, z: 10 }),
    order(2160, 2, ReferencePlan::AttackHero { slot: 3 }),
    order(2225, 0, ReferencePlan::Move { x: -30, z: -6 }),
];

/// The lanes: Cinder, player 0's hero, waits on the west lane north of its middle, and Veil,
/// player 5's, on the east lane south of it; each farms every 40 ticks from tick 2810, once the
/// first waves meet in tick 2803, to tick 3170.
const LANES: [Scripted<ReferencePlan>; 22] = {
    let mut script = [order(1200, 0, ReferencePlan::Move { x: -38, z: -8 }); 22];
    script[1] = order(1200, 5, ReferencePlan::Move { x: 38, z: 8 });
    let mut at = 0;
    while at < 10 {
        let stamp = 2810 + 40 * at as u64;
        script[2 + 2 * at] = order(stamp, 0, ReferencePlan::Farm);
        script[3 + 2 * at] = order(stamp, 5, ReferencePlan::Farm);
        at += 1;
    }
    script
};

const fn order(stamp: u64, slot: u32, plan: ReferencePlan) -> Scripted<ReferencePlan> {
    Scripted::new(stamp, slot, plan)
}

const fn attack_at((x, z): (i64, i64)) -> ReferencePlan {
    ReferencePlan::AttackSpawnedAt { x, z }
}

impl Reference3v3 {
    pub const PLAYERS: u32 = 6;

    pub fn load() -> Reference3v3 {
        let packages = ModePackages::from_dir(&PackageDir::workspace("moba/modes/3v3"))
            .unwrap_or_else(|error| panic!("{error}"));
        Reference3v3 {
            session: FixedSession::new(packages, TICK_HZ, Reference3v3::PLAYERS),
        }
    }

    pub const fn packages(&self) -> &ModePackages {
        self.session.packages()
    }

    /// The seed of the log's first segment.
    pub fn seed() -> ServerSeed {
        FixedSession::seed()
    }

    /// A match at tick 0, in which each player picked a hero, in slot order, and two spells.
    pub fn start(&self) -> FixedMatch {
        let mut fixed = self.session.start();
        for (slot, hero) in (0..).zip(HEROES) {
            let payload = ModeInput::payload(&[
                ModeInput {
                    name: "hero",
                    value: InputValue::String(hero),
                },
                ModeInput {
                    name: "spells",
                    value: InputValue::StringList(vec!["haste", "mend"]),
                },
            ]);
            fixed.send(slot, Tick::new(0), &payload);
        }
        fixed
    }

    /// Runs tick `tick` of `fixed`, a match of `play`: first sends its orders stamped for it,
    /// then runs it.
    pub fn play_tick(fixed: &mut FixedMatch, play: Play, tick: u64) {
        let script: &[Scripted<ReferencePlan>] = match play {
            Play::Camp => &CAMP,
            Play::Skirmish => &SKIRMISH,
            Play::Lanes => &LANES,
        };
        Scripted::play_tick(script, fixed, tick);
    }
}

impl Plan for ReferencePlan {
    fn order(self, units: MatchUnits<'_>, slot: u32) -> Order {
        let hero = units.hero(slot);
        let action = match self {
            ReferencePlan::Move { x, z } => Action::Move {
                x: Num::int(x),
                z: Num::int(z),
            },
            ReferencePlan::AttackSpawnedAt { x, z } => Action::Attack {
                target: units.spawned_at(x, z),
            },
            ReferencePlan::AttackHero { slot } => Action::Attack {
                target: units.hero(slot),
            },
            ReferencePlan::Cast { slot } => Action::Slot {
                slot,
                target: ActionTarget::None,
            },
            ReferencePlan::Farm => weakest_creep(units, hero).map_or_else(
                || {
                    let at = units.position(hero).get();
                    Action::Move { x: at.x, z: at.z }
                },
                |target| Action::Attack { target },
            ),
        };
        Order { unit: hero, action }
    }
}

/// The living creep of `hero`'s enemies, a unit that walks a lane, of least life within
/// `FARM_REACH` of it, by stable id on a tie.
fn weakest_creep(units: MatchUnits<'_>, hero: StableId) -> Option<StableId> {
    let world = units.world();
    let life = Combat::life(world).expect("the 3v3 has a life pool");
    let at = units.position(hero).get();
    let team = units
        .all()
        .find(|&(id, _)| id == hero)
        .and_then(|(_, unit)| unit.get::<Team>().copied())
        .expect("a hero has a team");
    units
        .all()
        .filter(|(_, unit)| {
            unit.contains::<PathWalker>()
                && !unit.contains::<Dead>()
                && unit.get::<Team>().is_some_and(|&other| other != team)
                && unit
                    .get::<Position>()
                    .is_some_and(|pos| pos.get().distance(at) <= Num::int(FARM_REACH))
        })
        .filter_map(|(id, unit)| Some((unit.get::<Pools>()?.current(life)?, id)))
        .min()
        .map(|(_, id)| id)
}
