use std::iter;

use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, UseGroup};
use campfire_capabilities::{Directive, Order};
use campfire_common::{DeclaredName, PlayerSlot, Tick};
use campfire_geometry::internals::{Density, KernelScene};
use campfire_items::{Inventory, ItemsDirective};
use campfire_math::Num;
use campfire_mode::{InputValue, ModeInput};
use campfire_navigation::PathWalker;
use campfire_orders::OrdersDirective;
use campfire_progression::ProgressionDirective;
use campfire_protocol::{PlayerSlotPlan, ServerInput};
use campfire_sim::{Position, StableId};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::internals::Spawn;
use campfire_units::{Dead, GroundPoint, Life};
use campfire_values::{ItemId, Names, Team};

use crate::fixed_session::FixedSession;
use crate::fixed_session::fixed_match::FixedMatch;
use crate::match_units::MatchUnits;
use crate::scripted::{CastAim, Plan, Scripted, TICK_HZ};
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
    /// The hero attacks player `player_slot`'s hero.
    AttackHero { player_slot: u32 },
    /// The hero casts the action in `slot` at `aim`.
    Cast { slot: u8, aim: CastAim },
    /// The hero uses the item `item` it carries in its first slot of it.
    Use { item: ItemId, aim: CastAim },
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

/// The experience that reaches level 6, the ultimate's first: the fifth of the 3v3's
/// `[tracks.level]`, whose first reaches level 2; and the gold the showcase's heroes start with,
/// past what any of them buys.
const SHOWCASE_XP: u32 = 2400;
const SHOWCASE_GOLD: u32 = 2000;
/// The ticks the showcase's heroes learn in, one rank each a tick, its ultimate first, from the
/// tick after their setup's timer gave them their experience: a learn of the basic of fewest
/// ranks reads the ranks as the tick before left them.
const SHOWCASE_LEARN: u64 = 21;

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
        Scripted::new(1200, 2, cast(HASTE, CastAim::Nothing)),
        Scripted::new(1201, 2, Moba3v3::Move { x: -34, z: 4 }),
        Scripted::new(1700, 0, Moba3v3::AttackHero { player_slot: 4 }),
        Scripted::new(1700, 1, Moba3v3::AttackHero { player_slot: 4 }),
        Scripted::new(1700, 4, Moba3v3::AttackHero { player_slot: 1 }),
        Scripted::new(1880, 1, cast(MEND, CastAim::Nothing)),
        Scripted::new(1880, 0, Moba3v3::Move { x: -36, z: 10 }),
        Scripted::new(2000, 4, Moba3v3::Move { x: -18, z: 24 }),
        Scripted::new(2160, 2, Moba3v3::AttackHero { player_slot: 3 }),
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

const fn cast(slot: u8, aim: CastAim) -> Moba3v3 {
    Moba3v3::Cast { slot, aim }
}

impl Moba3v3 {
    pub const PLAYERS: u32 = 6;
    /// The ticks the match plays: the skirmish, then the farm to its end, in tick 15,199, 1,780
    /// ticks past the learn of tick 13,420, the first after the last hero reaches level 6, in
    /// tick 13,284: a margin, as the last hero's farm moves with each change of movement or of
    /// the seed chain, whose segments the match's length sets.
    pub const FARM_END: u64 = 15_200;
    /// In the showcase's match, whose heroes spawn in tick 19, at the end of its pick of 1 s: the
    /// tick they shop in, at their fountain, behind their spawn; the tick the first cast is sent
    /// in, as they stand in the arena, each later one `CAST_EVERY` ticks after the one before;
    /// and how many casts it sends.
    pub const SHOP: u64 = 27;
    pub const SHOWCASE: u64 = 327;
    pub const CAST_EVERY: u64 = 40;
    pub const CASTS: u64 = 37;

    /// The script: the skirmish; then the farm, in which the heroes learn each rank their points
    /// allow.
    fn script() -> Vec<Scripted<Self>> {
        let mut script = SKIRMISH.to_vec();
        for stamp in (FARM..Self::FARM_END).step_by(FARM_EVERY) {
            script.extend(
                (0..)
                    .zip(POSTS)
                    .map(|(slot, post)| Scripted::new(stamp, slot, farm_at(post))),
            );
        }
        for stamp in (LEARN..Self::FARM_END).step_by(LEARN_EVERY) {
            for slot in 0..Self::PLAYERS {
                let ultimate = Self::Learn { slot: ULTIMATE };
                script.push(Scripted::new(stamp, slot, ultimate));
                script.push(Scripted::new(stamp, slot, Self::LearnBasic));
            }
        }
        script
    }

    /// The showcase's script, from its heroes at level 6 at their spawn: each learns its
    /// ultimate, then a rank of its basic of fewest ranks a tick, five of them; shops; walks to
    /// its place in the arena; and uses its items and casts each of its abilities, one cast every
    /// `CAST_EVERY` ticks. `item` names an item.
    fn showcase_script(item: &impl Fn(&str) -> ItemId) -> Vec<Scripted<Self>> {
        let mut script = Vec::new();
        for slot in 0..Self::PLAYERS {
            let ultimate = Self::Learn { slot: ULTIMATE };
            script.push(Scripted::new(SHOWCASE_LEARN, slot, ultimate));
            for basic in 1..=5 {
                script.push(Scripted::new(
                    SHOWCASE_LEARN + basic,
                    slot,
                    Self::LearnBasic,
                ));
            }
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
            script.push(Scripted::new(Self::SHOP, slot, Self::Buy { item }));
        }
        script.push(Scripted::new(Self::SHOP, 5, Self::Swap { from: 0, to: 1 }));
        script.push(Scripted::new(Self::SHOP + 1, 5, Self::Sell { slot: 1 }));
        for (slot, post) in (0..).zip(ARENA) {
            script.push(Scripted::new(Self::SHOP + 2, slot, walk_to(post)));
        }
        let casts = Self::casts(item);
        assert_eq!(casts.len() as u64, Self::CASTS);
        for (at, (slot, plan)) in (0..).zip(casts) {
            script.push(Scripted::new(
                Self::SHOWCASE + Self::CAST_EVERY * at,
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
    fn casts(item: &impl Fn(&str) -> ItemId) -> Vec<(u32, Self)> {
        let hero = |slot| CastAim::Hero { slot };
        let at = |slot| CastAim::HeroPoint { slot };
        let using = |name, aim| Self::Use {
            item: item(name),
            aim,
        };
        vec![
            (2, using("stoneplate", CastAim::Nothing)),
            (4, using("health_potion", CastAim::Nothing)),
            (5, using("health_potion", CastAim::Nothing)),
            (0, using("mana_potion", CastAim::Nothing)),
            (1, using("sight_ward", CastAim::Point { x: 0, z: -23 })),
            (1, using("vision_ward", CastAim::Point { x: 1, z: -23 })),
            (0, using("elixir_of_sight", CastAim::Nothing)),
            (5, cast(FIRST, hero(1))),
            (1, cast(SECOND, hero(3))),
            (3, using("quicksilver", CastAim::Nothing)),
            (5, cast(SECOND, CastAim::Nothing)),
            (1, cast(THIRD, hero(2))),
            (5, cast(THIRD, CastAim::Nothing)),
            (1, cast(FIRST, at(4))),
            (1, cast(FIRST, at(4))),
            (5, cast(ULTIMATE, hero(1))),
            (1, cast(ULTIMATE, CastAim::Nothing)),
            (0, cast(FIRST, at(3))),
            (2, cast(SECOND, CastAim::Nothing)),
            (4, cast(FIRST, CastAim::Nothing)),
            (4, Self::AttackHero { player_slot: 2 }),
            (3, cast(FIRST, hero(0))),
            (4, walk_to(ARENA[4])),
            (0, cast(SECOND, at(3))),
            (2, cast(THIRD, CastAim::Nothing)),
            (4, cast(SECOND, at(2))),
            (3, cast(SECOND, CastAim::Nothing)),
            (0, cast(THIRD, hero(5))),
            (2, cast(SECOND, CastAim::Nothing)),
            (4, cast(FIRST, CastAim::Nothing)),
            (2, cast(FIRST, at(5))),
            (4, cast(THIRD, CastAim::Point { x: 0, z: 0 })),
            (3, cast(THIRD, CastAim::Nothing)),
            (2, cast(ULTIMATE, CastAim::Nothing)),
            (0, cast(ULTIMATE, hero(3))),
            (4, cast(ULTIMATE, at(2))),
            (3, cast(ULTIMATE, CastAim::Nothing)),
        ]
    }

    pub fn load() -> ScriptedMode<Self> {
        let players = usize::try_from(Self::PLAYERS).expect("6 fits usize");
        Self::planned(iter::repeat_n(PlayerSlotPlan::Player, players))
    }

    /// The MOBA 3v3 with its slots opened as `plan` says.
    pub fn planned(plan: impl IntoIterator<Item = PlayerSlotPlan>) -> ScriptedMode<Self> {
        let packages = TestMode::Moba3v3.packages();
        let session = FixedSession::planned(packages, plan).rate(TICK_HZ);
        ScriptedMode::new(session, Self::script())
    }

    /// The showcase's match, a case of its own from its setup: the 3v3 whose pick lasts 1 s and
    /// whose waves come after the showcase, whose script gives each hero, by a timer of its
    /// setup in the tick after the heroes spawn, the experience of level 6 and the gold of its
    /// shop, played by `showcase_script` (02-engine-core.md, A rule's case starts from its setup).
    pub fn showcase() -> ScriptedMode<Self> {
        let picked = "        ctx.every(\"income\", ctx.params.income_ms);\n";
        let timed = format!("{picked}        ctx.after(\"setup\", 50);\n");
        let income = "    } else if name == \"income\" {\n";
        let setup = format!(
            "    }} else if name == \"setup\" {{\n        \
             for hero in ctx.all_avatars() {{\n            \
             ctx.add_xp(hero, \"level\", {SHOWCASE_XP});\n            \
             ctx.add_resource(hero.owner, \"gold\", {SHOWCASE_GOLD});\n        }}\n{income}"
        );
        let timers = "[\"income\", \"pick_end\", \"setup\", \"wave\"]";
        let mut tree = PackageTree::of(TestMode::Moba3v3);
        tree.edit_all([
            ("modes/3v3/data/mode.toml", Edit::Set("timers", timers)),
            (
                "modes/3v3/data/mode.toml",
                Edit::Set("params.pick_ms", "1000"),
            ),
            (
                "modes/3v3/data/mode.toml",
                Edit::Set("params.first_wave_ms", "600000"),
            ),
            ("modes/3v3/scripts/mode.rhai", Edit::Replace(picked, &timed)),
            ("modes/3v3/scripts/mode.rhai", Edit::Replace(income, &setup)),
        ]);
        let packages = tree.packages();
        let items = packages.mode().content.items();
        let items: Names<ItemId> = items.keys().map(DeclaredName::as_str).collect();
        let item = |name: &str| items.named(name).expect("an item of the 3v3");
        let script = Self::showcase_script(&item);
        let players = usize::try_from(Self::PLAYERS).expect("6 fits usize");
        let plan = iter::repeat_n(PlayerSlotPlan::Player, players);
        let session = FixedSession::planned(packages, plan).rate(TICK_HZ);
        ScriptedMode::new(session, script)
    }

    /// Spawns `KernelScene::UNITS` of the 3v3's creeps into `world`, on both teams in turn by
    /// pairs, at the crowded scene's points, which lie within its map.
    pub fn crowd_creeps(world: &mut World) {
        let mut scene = KernelScene::new(13);
        for unit in 0..KernelScene::UNITS {
            let position = scene.point(Density::Crowded.span());
            let team = Team::new(u8::from(unit % 4 >= 2));
            Spawn::named(world, CREEPS[unit % 2], position)
                .team(team)
                .place(world);
        }
    }
}

impl Plan for Moba3v3 {
    /// Each player and each bot picks a hero, in slot order, and two spells; an open player slot
    /// picks nothing.
    fn start(fixed: &mut FixedMatch, slots: &[PlayerSlotPlan]) {
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
                PlayerSlotPlan::Player => {
                    fixed.send(slot, Tick::new(0), &payload);
                }
                PlayerSlotPlan::Bot => {
                    let player_slot = PlayerSlot::new(slot);
                    let pick = ServerInput::Bot {
                        player_slot,
                        payload: &payload,
                    };
                    fixed.serve(pick).unwrap();
                }
                PlayerSlotPlan::Open => {}
            }
        }
    }

    fn order(self, units: MatchUnits<'_>, player_slot: u32) -> Order {
        let hero = units.hero(player_slot);
        let action = match self {
            Self::Move { x, z } => Directive::Orders(OrdersDirective::Move {
                to: GroundPoint {
                    x: Num::from(x),
                    z: Num::from(z),
                },
            }),
            Self::AttackSpawnedAt { x, z } => Directive::Orders(OrdersDirective::Attack {
                target: units.spawned_at((x, 0, z)),
            }),
            Self::AttackHero { player_slot } => Directive::Orders(OrdersDirective::Attack {
                target: units.hero(player_slot),
            }),
            Self::Cast { slot, aim } => Directive::Orders(OrdersDirective::Slot {
                slot,
                target: aim.target(units),
            }),
            Self::Use { item, aim } => Directive::Orders(OrdersDirective::Slot {
                slot: carried_at(units, hero, item),
                target: aim.target(units),
            }),
            Self::Learn { slot } => Directive::Progression(ProgressionDirective::Learn { slot }),
            Self::LearnBasic => Directive::Progression(ProgressionDirective::Learn {
                slot: fewest_ranks(units, hero),
            }),
            Self::Farm { x, z } => Directive::Orders(weakest_creep(units, hero).map_or_else(
                || OrdersDirective::Move {
                    to: GroundPoint {
                        x: Num::from(x),
                        z: Num::from(z),
                    },
                },
                |target| OrdersDirective::Attack { target },
            )),
            Self::Buy { item } => Directive::Items(ItemsDirective::Buy {
                item,
                shop: units.shop_of(hero),
            }),
            Self::Sell { slot } => Directive::Items(ItemsDirective::Sell {
                stack: slot,
                shop: units.shop_of(hero),
            }),
            Self::Swap { from, to } => Directive::Items(ItemsDirective::Swap { from, to }),
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
    let at = units.position(hero);
    let team = *units.unit(hero).get::<Team>().expect("a hero has a team");
    units
        .all()
        .filter(|(_, unit)| {
            unit.contains::<PathWalker>()
                && !unit.contains::<Dead>()
                && unit.get::<Team>().is_some_and(|&other| other != team)
                && unit
                    .get::<Position>()
                    .is_some_and(|position| position.get().distance(at) <= Num::int(FARM_REACH))
        })
        .filter_map(|(id, unit)| Some((unit.get::<Life>()?.current(), id)))
        .min()
        .map(|(_, id)| id)
}
