use std::iter;

use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, UseGroup};
use campfire_capabilities::{Action, Order};
use campfire_common::{PlayerSlot, Tick};
use campfire_geometry::internals::{Density, KernelScene};
use campfire_items::{Inventory, ItemId, ItemsAction};
use campfire_math::Num;
use campfire_mode::{InputValue, Mode, ModeInput};
use campfire_navigation::PathWalker;
use campfire_orders::OrdersAction;
use campfire_package_disk::internals::TestMode;
use campfire_progression::ProgressionAction;
use campfire_protocol::{InputRules, ServerInput, ServerSeeds, SlotPlan};
use campfire_sim::{Position, StableId};
use campfire_stats::Life;
use campfire_units::Dead;
use campfire_values::{DeclaredName, Names, Team};

use crate::fixed_session::FixedSession;
use crate::fixed_session::fixed_match::FixedMatch;
use crate::match_units::MatchUnits;
use crate::scripted::{Aim, Plan, Scripted, TICK_HZ};
use crate::scripted_mode::ScriptedMode;

pub(crate) mod bench;

/// The hero each slot picks.
const HEROES: [&str; 6] = [
    "hero-cinder",
    "hero-gale",
    "hero-husk",
    "hero-kensho",
    "hero-rime",
    "hero-veil",
];

/// The MOBA 3v3 as its packages hold it, at its slowest rate, which plays a match in the
/// fewest ticks, with six players of fixed keys. Its plan is what a 3v3 player orders, by the
/// role of the units it names.
#[derive(Debug, Clone, Copy)]
pub enum Moba3v3 {
    /// The hero walks to a point.
    Move { x: i16, z: i16 },
    /// The hero attacks the unit that spawned at a point: a camp's, or a structure.
    AttackSpawnedAt { x: i16, z: i16 },
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
    Farm { x: i16, z: i16 },
    /// The hero buys one `item` at the shop.
    Buy { item: ItemId },
    /// The hero sells the stack in its inventory slot `slot`.
    Sell { slot: u8 },
    /// The hero swaps its inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
}

/// The creep types of the 3v3's waves, which a crowd's units take in turn.
const CREEPS: [&str; 2] = ["melee_creep", "caster_creep"];

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

/// The south camp's wolf, west of the middle, on its marker.
const SOUTH_CAMP: (i16, i16) = (-18, 12);

/// Where each player's hero farms its lane: Cinder and Husk the west lane's north side, Kensho and
/// Rime its south side; Gale and Veil the east lane's.
const POSTS: [(i16, i16); 6] = [(-38, -8), (38, -8), (-36, -8), (-38, 8), (-36, 8), (38, 8)];
/// Where each player's hero stands in the jungle south of the north base as the heroes cast:
/// each north hero 6 m north of the south hero it faces.
const ARENA: [(i16, i16); 6] = [(-2, -26), (0, -26), (2, -26), (-2, -20), (0, -20), (2, -20)];
/// The ticks the heroes farm in, every `FARM_EVERY` ticks from `FARM`, and learn in, every
/// `LEARN_EVERY` from `LEARN`: never in a farm's tick, as a player sends at most four inputs a
/// tick.
const FARM: u64 = 3200;
const FARM_EVERY: usize = 40;
const LEARN: u64 = 3220;
const LEARN_EVERY: usize = 200;
/// The tick the heroes walk home in, to shop in `Moba3v3::SHOP`: after the learn of tick 13,020,
/// the first after the last hero reaches level 6, in tick 12,888, as the showcase needs each
/// hero's ultimate, which the 3v3's farm check asserts. Husk walks the west lane home from its
/// post and reaches its fountain's reach within the 800 ticks before the shop.
const RECALL: u64 = 13_200;

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
const SKIRMISH: [Scripted<Moba3v3>; 45] = {
    let mut script = [Scripted::new(1200, 0, Moba3v3::Move { x: -2, z: -26 }); 45];
    let moves = [
        Scripted::new(1200, 1, Moba3v3::Move { x: 2, z: -26 }),
        Scripted::new(1200, 4, Moba3v3::Move { x: 0, z: -20 }),
        Scripted::new(1200, 3, Moba3v3::Move { x: -38, z: 12 }),
        Scripted::new(1200, 5, Moba3v3::Move { x: 38, z: 8 }),
        Scripted::new(1200, 2, cast(HASTE, Aim::Nothing)),
        Scripted::new(1201, 2, Moba3v3::Move { x: -34, z: 4 }),
        Scripted::new(1700, 0, Moba3v3::AttackHero { slot: 4 }),
        Scripted::new(1700, 1, Moba3v3::AttackHero { slot: 4 }),
        Scripted::new(1700, 4, Moba3v3::AttackHero { slot: 1 }),
        Scripted::new(1880, 1, cast(MEND, Aim::Nothing)),
        Scripted::new(1880, 0, Moba3v3::Move { x: -36, z: 10 }),
        Scripted::new(2000, 4, Moba3v3::Move { x: -18, z: 24 }),
        Scripted::new(2160, 2, Moba3v3::AttackHero { slot: 3 }),
        Scripted::new(2225, 0, Moba3v3::Move { x: -30, z: -6 }),
        Scripted::new(2240, 0, Moba3v3::Move { x: -38, z: -8 }),
        Scripted::new(2320, 3, attack_at(SOUTH_CAMP)),
        Scripted::new(2500, 3, Moba3v3::Move { x: -18, z: 30 }),
        Scripted::new(2650, 3, attack_at(SOUTH_CAMP)),
        Scripted::new(2650, 4, attack_at(SOUTH_CAMP)),
        Scripted::new(1200, 0, Moba3v3::Learn { slot: FIRST }),
        Scripted::new(1200, 0, Moba3v3::Learn { slot: SECOND }),
        Scripted::new(1200, 5, Moba3v3::Learn { slot: ULTIMATE }),
        Scripted::new(1201, 5, Moba3v3::Learn { slot: FIRST }),
        Scripted::new(1900, 4, Moba3v3::Learn { slot: SECOND }),
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
        script[farms + 2 * farm] = Scripted::new(stamp, 0, farm_at(POSTS[0]));
        script[farms + 1 + 2 * farm] = Scripted::new(stamp, 5, farm_at(POSTS[5]));
        farm += 1;
    }
    script
};

const fn attack_at((x, z): (i16, i16)) -> Moba3v3 {
    Moba3v3::AttackSpawnedAt { x, z }
}

const fn walk_to((x, z): (i16, i16)) -> Moba3v3 {
    Moba3v3::Move { x, z }
}

const fn farm_at((x, z): (i16, i16)) -> Moba3v3 {
    Moba3v3::Farm { x, z }
}

const fn cast(slot: u8, aim: Aim) -> Moba3v3 {
    Moba3v3::Cast { slot, aim }
}

impl Moba3v3 {
    pub const PLAYERS: u32 = 6;
    /// The tick the heroes shop in; the tick the showcase's first cast is sent in, each later
    /// one `CAST_EVERY` ticks after the one before; and how many casts it sends.
    pub const SHOP: u64 = 14_000;
    pub const SHOWCASE: u64 = 14_300;
    pub const CAST_EVERY: u64 = 40;
    pub const CASTS: u64 = 37;

    /// The script: the skirmish; the farm, in which the heroes learn each rank their points
    /// allow; the walk home, the shop, and the showcase, in which each hero uses its items and
    /// casts each of its abilities, one cast every `CAST_EVERY` ticks. `item` names an
    /// item.
    fn script(item: &impl Fn(&str) -> ItemId) -> Vec<Scripted<Moba3v3>> {
        let mut script = SKIRMISH.to_vec();
        for stamp in (FARM..RECALL).step_by(FARM_EVERY) {
            script.extend(
                (0..)
                    .zip(POSTS)
                    .map(|(slot, post)| Scripted::new(stamp, slot, farm_at(post))),
            );
        }
        for stamp in (LEARN..Moba3v3::SHOWCASE).step_by(LEARN_EVERY) {
            for slot in 0..Moba3v3::PLAYERS {
                let ultimate = Moba3v3::Learn { slot: ULTIMATE };
                script.push(Scripted::new(stamp, slot, ultimate));
                script.push(Scripted::new(stamp, slot, Moba3v3::LearnBasic));
            }
        }
        for slot in 0..Moba3v3::PLAYERS {
            let z = if slot < 3 { -60 } else { 60 };
            script.push(Scripted::new(RECALL, slot, Moba3v3::Move { x: 0, z }));
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
            script.push(Scripted::new(Moba3v3::SHOP, slot, Moba3v3::Buy { item }));
        }
        script.push(Scripted::new(
            Moba3v3::SHOP,
            5,
            Moba3v3::Swap { from: 0, to: 1 },
        ));
        script.push(Scripted::new(
            Moba3v3::SHOP + 1,
            5,
            Moba3v3::Sell { slot: 1 },
        ));
        for (slot, post) in (0..).zip(ARENA) {
            script.push(Scripted::new(Moba3v3::SHOP + 2, slot, walk_to(post)));
        }
        let showcase = Moba3v3::showcase(item);
        assert_eq!(showcase.len() as u64, Moba3v3::CASTS);
        for (at, (slot, plan)) in (0..).zip(showcase) {
            script.push(Scripted::new(
                Moba3v3::SHOWCASE + Moba3v3::CAST_EVERY * at,
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
    fn showcase(item: &impl Fn(&str) -> ItemId) -> Vec<(u32, Moba3v3)> {
        let hero = |slot| Aim::Hero { slot };
        let at = |slot| Aim::HeroPoint { slot };
        let using = |name, aim| Moba3v3::Use {
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
            (4, Moba3v3::AttackHero { slot: 2 }),
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
            (4, cast(THIRD, Aim::Point { x: 0, z: 15 })),
            (3, cast(THIRD, Aim::Nothing)),
            (2, cast(ULTIMATE, Aim::Nothing)),
            (0, cast(ULTIMATE, hero(3))),
            (4, cast(ULTIMATE, at(2))),
            (3, cast(ULTIMATE, Aim::Nothing)),
        ]
    }

    pub fn load() -> ScriptedMode<Moba3v3> {
        let players = usize::try_from(Moba3v3::PLAYERS).expect("6 fits usize");
        Moba3v3::planned(iter::repeat_n(SlotPlan::Player, players))
    }

    /// The MOBA 3v3 with its slots opened as `plan` says.
    pub fn planned(plan: impl IntoIterator<Item = SlotPlan>) -> ScriptedMode<Moba3v3> {
        let packages = TestMode::Moba3v3.packages();
        let items = &packages.mode().content.items;
        let items: Names<ItemId> = items.keys().map(DeclaredName::as_str).collect();
        let item = |name: &str| items.named(name).expect("an item of the 3v3");
        let script = Moba3v3::script(&item);
        let session = FixedSession::planned(packages, TICK_HZ, InputRules::ROOMY, plan);
        ScriptedMode::new(session, script)
    }

    /// Spawns `KernelScene::UNITS` of the 3v3's creeps into `world`, on both teams in turn by
    /// pairs, at the crowded scene's points, which lie within its map.
    pub fn crowd_creeps(world: &mut World) {
        let mut scene = KernelScene::new(13);
        for unit in 0..KernelScene::UNITS {
            let pos = scene.point(Density::Crowded.span());
            let team = Team::new(u8::from(unit % 4 >= 2));
            Mode::spawn_typed(world, CREEPS[unit % 2], pos, team, ());
        }
    }

    /// Every segment's seed, as the server knows them.
    pub const fn seeds() -> ServerSeeds {
        FixedSession::seeds()
    }
}

impl Plan for Moba3v3 {
    /// Each player and each bot picks a hero, in slot order, and two spells; an open slot picks
    /// nothing.
    fn start(fixed: &mut FixedMatch, slots: &[SlotPlan]) {
        for ((slot, hero), plan) in (0..).zip(HEROES).zip(slots) {
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
                    fixed.serve(pick).unwrap();
                }
                SlotPlan::Open => {}
            }
        }
    }

    fn order(self, units: MatchUnits<'_>, slot: u32) -> Order {
        let hero = units.hero(slot);
        let action = match self {
            Moba3v3::Move { x, z } => Action::Orders(OrdersAction::Move {
                x: Num::from(x),
                z: Num::from(z),
            }),
            Moba3v3::AttackSpawnedAt { x, z } => Action::Orders(OrdersAction::Attack {
                target: units.spawned_at((x, 0, z)),
            }),
            Moba3v3::AttackHero { slot } => Action::Orders(OrdersAction::Attack {
                target: units.hero(slot),
            }),
            Moba3v3::Cast { slot, aim } => Action::Orders(OrdersAction::Slot {
                slot,
                target: aim.target(units),
            }),
            Moba3v3::Use { item, aim } => Action::Orders(OrdersAction::Slot {
                slot: carried_at(units, hero, item),
                target: aim.target(units),
            }),
            Moba3v3::Learn { slot } => Action::Progression(ProgressionAction::Learn { slot }),
            Moba3v3::LearnBasic => Action::Progression(ProgressionAction::Learn {
                slot: fewest_ranks(units, hero),
            }),
            Moba3v3::Farm { x, z } => Action::Orders(weakest_creep(units, hero).map_or_else(
                || OrdersAction::Move {
                    x: Num::from(x),
                    z: Num::from(z),
                },
                |target| OrdersAction::Attack { target },
            )),
            Moba3v3::Buy { item } => Action::Items(ItemsAction::Buy {
                item,
                shop: units.shop_of(hero),
            }),
            Moba3v3::Sell { slot } => Action::Items(ItemsAction::Sell {
                slot,
                shop: units.shop_of(hero),
            }),
            Moba3v3::Swap { from, to } => Action::Items(ItemsAction::Swap { from, to }),
        };
        Order::one(hero, action)
    }
}

/// The slot of `hero`'s basic ability of fewest ranks, the first of them on a tie.
fn fewest_ranks(units: MatchUnits<'_>, hero: StableId) -> u8 {
    let slots = units
        .unit(hero)
        .get::<ActionSlots>()
        .expect("a hero has slots");
    [FIRST, SECOND, THIRD]
        .into_iter()
        .min_by_key(|&slot| slots.slot(slot).expect("a basic ability").rank)
        .expect("three basic abilities")
}

/// The action slot of `hero`'s inventory slot that holds `item` first: the inventory's slots
/// begin at the first slot of its kind.
fn carried_at(units: MatchUnits<'_>, hero: StableId, item: ItemId) -> u8 {
    let unit = units.unit(hero);
    let inventory = unit.get::<Inventory>().expect("a hero carries");
    let group = unit
        .get::<UseGroup>()
        .expect("a carrier's inventory has its kind");
    let slots = unit.get::<ActionSlots>().expect("a hero has slots");
    let at = inventory
        .slots()
        .iter()
        .position(|carried| carried.is_some_and(|carried| carried.item == item));
    let at = u8::try_from(at.expect("the hero carries the item")).expect("an inventory slot");
    slots.first_of(group.kind()) + at
}

/// The living creep of `hero`'s enemies, a unit that walks a lane, of least life within
/// `FARM_REACH` of it, by stable id on a tie.
fn weakest_creep(units: MatchUnits<'_>, hero: StableId) -> Option<StableId> {
    let at = units.pos(hero);
    let team = *units.unit(hero).get::<Team>().expect("a hero has a team");
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
        .filter_map(|(id, unit)| Some((unit.get::<Life>()?.current(), id)))
        .min()
        .map(|(_, id)| id)
}
