use std::num::NonZeroU32;

use campfire_capabilities::{
    Action, ActionSlots, ActionTarget, Combat, Dead, InputValue, Inventory, ItemId, ModeInput,
    Order, PathWalker, Pools, Team,
};
use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::{ServerInput, ServerSeeds, SlotPlan};
use campfire_sim::{Position, StableId};

use crate::fixed_match::FixedMatch;
use crate::fixed_session::FixedSession;
use crate::input_rules::InputRules;
use crate::match_units::MatchUnits;
use crate::scripted::{Plan, Scripted};

/// The reference 3v3 as its packages hold it, at its slowest rate, which plays a match in the
/// fewest ticks, with six players of fixed keys.
#[derive(Debug)]
pub struct Reference3v3 {
    session: FixedSession,
    script: Vec<Scripted<ReferencePlan>>,
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
    /// The hero casts the action in `slot` at `aim`.
    Cast { slot: u8, aim: Aim },
    /// The hero uses the item `item` it carries in its first slot of it.
    Use { item: ItemId, aim: Aim },
    /// The hero learns the next rank of the ability in `slot`.
    Learn { slot: u8 },
    /// The hero learns the next rank of its basic ability of fewest ranks, the first of them on a
    /// tie.
    LearnBasic,
    /// The hero attacks the enemy creep of least life within `FARM_REACH` of it, by stable id on
    /// a tie; with none, it walks to its post at `x`, `z`.
    Farm { x: i64, z: i64 },
    /// The hero buys one `item` at the shop.
    Buy { item: ItemId },
    /// The hero sells the stack in its inventory slot `slot`.
    Sell { slot: u8 },
    /// The hero swaps its inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
}

/// What a cast aims at.
#[derive(Debug, Clone, Copy)]
enum Aim {
    Nothing,
    /// Player `slot`'s hero.
    Hero {
        slot: u32,
    },
    /// The point player `slot`'s hero stands at as the order is sent.
    HeroPoint {
        slot: u32,
    },
    Point {
        x: i64,
        z: i64,
    },
}

/// How far from a hero a creep it farms stands at most, in meters.
const FARM_REACH: i64 = 10;

/// A hero's slots of its three basic abilities and of its ultimate; and of the spells `haste` and
/// `mend`, as each player picks them, after them.
const FIRST: u8 = 0;
const SECOND: u8 = 1;
const THIRD: u8 = 2;
const ULTIMATE: u8 = 3;
const HASTE: u8 = 4;
const MEND: u8 = 5;
/// A hero's first action slot of its inventory, after its spells and its weapon.
const INVENTORY: u8 = 7;

/// The south camp's wolf, west of the middle, on its marker.
const SOUTH_CAMP: (i64, i64) = (-18, 12);

/// Where each player's hero farms its lane: Cinder and Husk the west lane's north side, Kensho and
/// Rime its south side; Gale and Veil the east lane's.
const POSTS: [(i64, i64); 6] = [(-38, -8), (38, -8), (-36, -8), (-38, 8), (-36, 8), (38, 8)];
/// Where each player's hero stands in the jungle south of the north base as the heroes cast:
/// each north hero 6 m north of the south hero it faces.
const ARENA: [(i64, i64); 6] = [(-2, -26), (0, -26), (2, -26), (-2, -20), (0, -20), (2, -20)];
/// The ticks the heroes farm in, every `FARM_EVERY` ticks from `FARM`, and learn in, every
/// `LEARN_EVERY` from `LEARN`: never in a farm's tick, as a player sends at most four inputs a
/// tick.
const FARM: u64 = 3200;
const FARM_EVERY: usize = 40;
const LEARN: u64 = 3220;
const LEARN_EVERY: usize = 200;
/// The tick the heroes walk home in, to shop in `Reference3v3::SHOP`.
const RECALL: u64 = 13_400;

/// The orders of the 3v3's scripted players after the pick, to the farm. Players 0 to 2 hold
/// Cinder, Gale and Husk, north; 3 to 5 Kensho, Rime and Veil, south. At 20 ticks a second the
/// heroes spawn in tick 1199 and the first waves meet in tick 2803.
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
const SKIRMISH: [Scripted<ReferencePlan>; 45] = {
    let mut script = [order(1200, 0, ReferencePlan::Move { x: -2, z: -26 }); 45];
    let moves = [
        order(1200, 1, ReferencePlan::Move { x: 2, z: -26 }),
        order(1200, 4, ReferencePlan::Move { x: 0, z: -20 }),
        order(1200, 3, ReferencePlan::Move { x: -38, z: 12 }),
        order(1200, 5, ReferencePlan::Move { x: 38, z: 8 }),
        order(1200, 2, cast(HASTE, Aim::Nothing)),
        order(1201, 2, ReferencePlan::Move { x: -34, z: 4 }),
        order(1700, 0, ReferencePlan::AttackHero { slot: 4 }),
        order(1700, 1, ReferencePlan::AttackHero { slot: 4 }),
        order(1700, 4, ReferencePlan::AttackHero { slot: 1 }),
        order(1880, 1, cast(MEND, Aim::Nothing)),
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
        script[farms + 2 * farm] = order(stamp, 0, farm_at(POSTS[0]));
        script[farms + 1 + 2 * farm] = order(stamp, 5, farm_at(POSTS[5]));
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

const fn walk_to((x, z): (i64, i64)) -> ReferencePlan {
    ReferencePlan::Move { x, z }
}

const fn farm_at((x, z): (i64, i64)) -> ReferencePlan {
    ReferencePlan::Farm { x, z }
}

const fn cast(slot: u8, aim: Aim) -> ReferencePlan {
    ReferencePlan::Cast { slot, aim }
}

impl Reference3v3 {
    pub const PLAYERS: u32 = 6;
    /// The tick the heroes shop in; the tick the showcase's first cast is sent in, each later
    /// one `CAST_EVERY` ticks after the one before; and how many casts it sends.
    pub const SHOP: u64 = 13_900;
    pub const SHOWCASE: u64 = 14_200;
    pub const CAST_EVERY: u64 = 40;
    pub const CASTS: u64 = 37;

    /// The script: the skirmish; the farm, in which the heroes learn each rank their points
    /// allow; the walk home, the shop, and the showcase, in which each hero uses its items and
    /// casts each of its abilities, one cast every `CAST_EVERY` ticks. `item` names an
    /// item.
    fn script(item: &impl Fn(&str) -> ItemId) -> Vec<Scripted<ReferencePlan>> {
        let mut script = SKIRMISH.to_vec();
        for stamp in (FARM..RECALL).step_by(FARM_EVERY) {
            script.extend(
                (0..)
                    .zip(POSTS)
                    .map(|(slot, post)| order(stamp, slot, farm_at(post))),
            );
        }
        for stamp in (LEARN..Reference3v3::SHOWCASE).step_by(LEARN_EVERY) {
            for slot in 0..Reference3v3::PLAYERS {
                let ultimate = ReferencePlan::Learn { slot: ULTIMATE };
                script.push(order(stamp, slot, ultimate));
                script.push(order(stamp, slot, ReferencePlan::LearnBasic));
            }
        }
        for slot in 0..Reference3v3::PLAYERS {
            let z = if slot < 3 { -60 } else { 60 };
            script.push(order(RECALL, slot, ReferencePlan::Move { x: 0, z }));
        }
        let buys = [
            (2, "cloth_armor"),
            (2, "ruby_crystal"),
            (2, "stoneplate"),
            (3, "cloth_armor"),
            (3, "quicksilver"),
            (1, "sight_ward"),
            (1, "vision_ward"),
            (4, "health_potion"),
            (0, "mana_potion"),
            (0, "elixir_of_sight"),
            (5, "long_sword"),
            (5, "health_potion"),
        ];
        for (slot, name) in buys {
            let item = item(name);
            script.push(order(Reference3v3::SHOP, slot, ReferencePlan::Buy { item }));
        }
        script.push(order(
            Reference3v3::SHOP,
            5,
            ReferencePlan::Swap { from: 0, to: 1 },
        ));
        script.push(order(
            Reference3v3::SHOP + 1,
            5,
            ReferencePlan::Sell { slot: 1 },
        ));
        for (slot, post) in (0..).zip(ARENA) {
            script.push(order(Reference3v3::SHOP + 2, slot, walk_to(post)));
        }
        let showcase = Reference3v3::showcase(item);
        assert_eq!(showcase.len() as u64, Reference3v3::CASTS);
        for (at, (slot, plan)) in (0..).zip(showcase) {
            script.push(order(
                Reference3v3::SHOWCASE + Reference3v3::CAST_EVERY * at,
                slot,
                plan,
            ));
        }
        script
    }

    /// The showcase's casts, by player slot, in order: first the items, then each hero's
    /// abilities, the teams by turns. Players 0 to 2 hold Cinder, Gale and Husk; 3 to 5 Kensho,
    /// Rime and Veil. Veil and Gale cast all theirs first, before the others' blows could fell
    /// them; Rime strikes Husk with Chill Arrows on, then walks back to her place, which
    /// ends her attack.
    fn showcase(item: &impl Fn(&str) -> ItemId) -> Vec<(u32, ReferencePlan)> {
        let hero = |slot| Aim::Hero { slot };
        let at = |slot| Aim::HeroPoint { slot };
        let using = |name, aim| ReferencePlan::Use {
            item: item(name),
            aim,
        };
        vec![
            (2, using("stoneplate", Aim::Nothing)),
            (4, using("health_potion", Aim::Nothing)),
            (5, using("health_potion", Aim::Nothing)),
            (0, using("mana_potion", Aim::Nothing)),
            (1, using("sight_ward", Aim::Point { x: 0, z: -23 })),
            (1, using("vision_ward", Aim::Point { x: 1, z: -23 })),
            (0, using("elixir_of_sight", Aim::Nothing)),
            (5, cast(FIRST, hero(1))),
            (1, cast(SECOND, hero(3))),
            (3, using("quicksilver", Aim::Nothing)),
            (5, cast(SECOND, Aim::Nothing)),
            (1, cast(THIRD, hero(2))),
            (5, cast(THIRD, Aim::Nothing)),
            (1, cast(FIRST, at(4))),
            (1, cast(FIRST, at(4))),
            (5, cast(ULTIMATE, hero(1))),
            (1, cast(ULTIMATE, Aim::Nothing)),
            (0, cast(FIRST, at(3))),
            (2, cast(SECOND, Aim::Nothing)),
            (4, cast(FIRST, Aim::Nothing)),
            (4, ReferencePlan::AttackHero { slot: 2 }),
            (3, cast(FIRST, hero(0))),
            (4, walk_to(ARENA[4])),
            (0, cast(SECOND, at(4))),
            (2, cast(THIRD, Aim::Nothing)),
            (4, cast(SECOND, at(2))),
            (3, cast(SECOND, Aim::Nothing)),
            (0, cast(THIRD, hero(5))),
            (2, cast(SECOND, Aim::Nothing)),
            (4, cast(FIRST, Aim::Nothing)),
            (2, cast(FIRST, at(5))),
            (4, cast(THIRD, Aim::Point { x: 0, z: 10 })),
            (3, cast(THIRD, Aim::Nothing)),
            (2, cast(ULTIMATE, Aim::Nothing)),
            (0, cast(ULTIMATE, hero(3))),
            (4, cast(ULTIMATE, at(2))),
            (3, cast(ULTIMATE, Aim::Nothing)),
        ]
    }

    pub fn load() -> Reference3v3 {
        let players = usize::try_from(Reference3v3::PLAYERS).expect("6 fits usize");
        Reference3v3::planned(vec![SlotPlan::Player; players])
    }

    /// The reference 3v3 with its slots opened as `plan` says.
    pub fn planned(plan: Vec<SlotPlan>) -> Reference3v3 {
        let packages = ModePackages::from_dir(&PackageDir::workspace("moba/modes/3v3"))
            .unwrap_or_else(|error| panic!("{error}"));
        let items = &packages.packages().next().expect("a mode").content.items;
        let item = |name: &str| ItemId::named(items, name).expect("an item of the 3v3");
        let script = Reference3v3::script(&item);
        Reference3v3 {
            session: FixedSession::planned(packages, TICK_HZ, InputRules::ROOMY, plan),
            script,
        }
    }

    pub const fn packages(&self) -> &ModePackages {
        self.session.packages()
    }

    /// Every segment's seed, as the server knows them.
    pub const fn seeds() -> ServerSeeds {
        FixedSession::seeds()
    }

    /// A match at tick 0, in which each player and each bot picked a hero, in slot order, and
    /// two spells; an open slot picked nothing.
    pub fn start(&self) -> FixedMatch {
        let mut fixed = self.session.start();
        let plan = &self.session.terms().slots;
        for ((slot, hero), plan) in (0..).zip(HEROES).zip(plan) {
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
            match plan {
                SlotPlan::Player => {
                    fixed.send(slot, Tick::new(0), &payload);
                }
                SlotPlan::Bot => {
                    let slot = PlayerSlot::new(slot);
                    let pick = ServerInput::Bot {
                        slot,
                        payload: &payload,
                    };
                    fixed.serve(pick).unwrap_or_else(|error| panic!("{error}"));
                }
                SlotPlan::Open => {}
            }
        }
        fixed
    }

    /// Runs tick `tick` of `fixed`: first sends the scripted players' orders stamped for it,
    /// then runs it.
    pub fn play_tick(&self, fixed: &mut FixedMatch, tick: u64) {
        Scripted::play_tick(&self.script, fixed, tick);
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
            ReferencePlan::Cast { slot, aim } => Action::Slot {
                slot,
                target: aim.target(units),
            },
            ReferencePlan::Use { item, aim } => Action::Slot {
                slot: INVENTORY + carried_at(units, hero, item),
                target: aim.target(units),
            },
            ReferencePlan::Learn { slot } => Action::Learn { slot },
            ReferencePlan::LearnBasic => Action::Learn {
                slot: fewest_ranks(units, hero),
            },
            ReferencePlan::Farm { x, z } => weakest_creep(units, hero).map_or(
                Action::Move {
                    x: Num::int(x),
                    z: Num::int(z),
                },
                |target| Action::Attack { target },
            ),
            ReferencePlan::Buy { item } => Action::Buy { item },
            ReferencePlan::Sell { slot } => Action::Sell { slot },
            ReferencePlan::Swap { from, to } => Action::Swap { from, to },
        };
        Order { unit: hero, action }
    }
}

impl Aim {
    fn target(self, units: MatchUnits<'_>) -> ActionTarget {
        match self {
            Aim::Nothing => ActionTarget::None,
            Aim::Hero { slot } => ActionTarget::Unit(units.hero(slot)),
            Aim::HeroPoint { slot } => ActionTarget::Point(units.position(units.hero(slot))),
            Aim::Point { x, z } => {
                let at = Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
                ActionTarget::Point(Position::new(at).expect("a map point"))
            }
        }
    }
}

/// The slot of `hero`'s basic ability of fewest ranks, the first of them on a tie.
fn fewest_ranks(units: MatchUnits<'_>, hero: StableId) -> u8 {
    let (_, unit) = units.all().find(|&(id, _)| id == hero).expect("a hero");
    let slots = unit.get::<ActionSlots>().expect("a hero has slots");
    [FIRST, SECOND, THIRD]
        .into_iter()
        .min_by_key(|&slot| slots.slot(slot).expect("a basic ability").rank)
        .expect("three basic abilities")
}

/// The inventory slot of `hero` that holds `item` first.
fn carried_at(units: MatchUnits<'_>, hero: StableId, item: ItemId) -> u8 {
    let (_, unit) = units.all().find(|&(id, _)| id == hero).expect("a hero");
    let inventory = unit.get::<Inventory>().expect("a hero carries");
    let at = inventory
        .slots()
        .iter()
        .position(|carried| carried.is_some_and(|carried| carried.item == item));
    u8::try_from(at.expect("the hero carries the item")).expect("an inventory slot")
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
