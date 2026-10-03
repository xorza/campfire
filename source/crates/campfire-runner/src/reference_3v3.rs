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
    /// The hero learns the next rank of the ability in `slot`.
    Learn { slot: u8 },
    /// The hero attacks the enemy creep of least life within `FARM_REACH` of it, by stable id on
    /// a tie; with none, it stands where it is.
    Farm,
}

/// How far from a hero a creep it farms stands at most, in meters.
const FARM_REACH: i64 = 10;

/// A hero's slots of its first two basic abilities and of its ultimate; and of the spells `haste`
/// and `mend`, as each player picks them, after its three basic abilities and its ultimate.
const FIRST: u8 = 0;
const SECOND: u8 = 1;
const ULTIMATE: u8 = 3;
const HASTE: u8 = 4;
const MEND: u8 = 5;

/// The south camp's wolf, west of the middle, on its marker.
const SOUTH_CAMP: (i64, i64) = (-18, 12);

/// The orders of the 3v3's scripted players after the pick. Players 0 to 2 hold Cinder, Gale and
/// Husk, north; 3 to 5 Kensho, Rime and Veil, south. At 20 ticks a second the heroes spawn in
/// tick 1199 and the first waves meet in tick 2803.
/// - A skirmish: Husk casts haste; Cinder and Gale fell Rime in the middle as she strikes Gale,
///   and Gale casts mend. Cinder walks into the range of the south's west outer tower, Husk
///   strikes Kensho beside it, which turns the tower on him, and he falls to it.
/// - A camp: Kensho strikes the south camp's wolf and walks south past its leash, which sends
///   it home; then he and Rime fell it.
/// - The lanes: Cinder on the west lane and Veil on the east one strike the enemy creep of least
///   life near them every 40 ticks while the first waves fight.
/// - Learning, with the point each hero spawns with: Cinder learns Fire Lance, and a second
///   learn in the tick finds no point; Veil's ultimate needs level 6, and she learns Dusk Mark
///   instead; Rime learns Fan of Frost while she is dead. None of these abilities holds a
///   passive, and no hero casts one.
const SCRIPT: [Scripted<ReferencePlan>; 45] = {
    let mut script = [order(1200, 0, ReferencePlan::Move { x: -2, z: -26 }); 45];
    let moves = [
        order(1200, 1, ReferencePlan::Move { x: 2, z: -26 }),
        order(1200, 4, ReferencePlan::Move { x: 0, z: -20 }),
        order(1200, 3, ReferencePlan::Move { x: -38, z: 12 }),
        order(1200, 5, ReferencePlan::Move { x: 38, z: 8 }),
        order(1200, 2, ReferencePlan::Cast { slot: HASTE }),
        order(1201, 2, ReferencePlan::Move { x: -34, z: 4 }),
        order(1700, 0, ReferencePlan::AttackHero { slot: 4 }),
        order(1700, 1, ReferencePlan::AttackHero { slot: 4 }),
        order(1700, 4, ReferencePlan::AttackHero { slot: 1 }),
        order(1880, 1, ReferencePlan::Cast { slot: MEND }),
        order(1880, 0, ReferencePlan::Move { x: -36, z: 10 }),
        order(2000, 4, ReferencePlan::Move { x: -18, z: 24 }),
        order(2160, 2, ReferencePlan::AttackHero { slot: 3 }),
        order(2225, 0, ReferencePlan::Move { x: -30, z: -6 }),
        order(2240, 0, ReferencePlan::Move { x: -38, z: -8 }),
        order(2320, 3, attack_at(SOUTH_CAMP)),
        order(2500, 3, ReferencePlan::Move { x: -18, z: 30 }),
        order(2650, 3, attack_at(SOUTH_CAMP)),
        order(2650, 4, attack_at(SOUTH_CAMP)),
        order(1200, 0, ReferencePlan::Learn { slot: FIRST }),
        order(1200, 0, ReferencePlan::Learn { slot: SECOND }),
        order(1200, 5, ReferencePlan::Learn { slot: ULTIMATE }),
        order(1201, 5, ReferencePlan::Learn { slot: FIRST }),
        order(1900, 4, ReferencePlan::Learn { slot: SECOND }),
    ];
    let mut at = 0;
    while at < moves.len() {
        script[1 + at] = moves[at];
        at += 1;
    }
    let farms = 1 + moves.len();
    let mut farm = 0;
    while farm < 10 {
        let stamp = 2810 + 40 * farm as u64;
        script[farms + 2 * farm] = order(stamp, 0, ReferencePlan::Farm);
        script[farms + 1 + 2 * farm] = order(stamp, 5, ReferencePlan::Farm);
        farm += 1;
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

    /// Runs tick `tick` of `fixed`: first sends the scripted players' orders stamped for it,
    /// then runs it.
    pub fn play_tick(fixed: &mut FixedMatch, tick: u64) {
        Scripted::play_tick(&SCRIPT, fixed, tick);
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
            ReferencePlan::Learn { slot } => Action::Learn { slot },
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
