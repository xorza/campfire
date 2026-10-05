//! The 3v3 after the skirmish: the heroes farm their lanes to level 6, learning a rank with each
//! point; walk home and shop; and in the showcase use their items and cast each of their
//! abilities once, each cast paying its cost and starting its cooldown at its rank, as the
//! heroes' and the mode's data give them.

use bevy_ecs::world::World;
use campfire_capabilities::internals;
use campfire_capabilities::{
    ActionData, ActionSlot, ActionSlots, Carried, DeclaredName, Inventory, ItemId, Level, Lifespan,
    ModifierId, Number, Points, PoolId, Pools, Ranked, Scalar, Stats, Team, Toggle,
};
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_runner::internals::{MatchUnits, Reference3v3};
use campfire_sim::{EntityIndex, Position, TickRate};

use crate::reference::gold;

/// Each player's hero's package and its abilities, slot by slot: Cinder, Gale and Husk north;
/// Kensho, Rime and Veil south.
const ABILITIES: [(&str, [&str; 4]); 6] = [
    (
        "hero-cinder",
        ["fire_lance", "eruption", "wildfire", "chain_fire"],
    ),
    ("hero-gale", ["cyclone", "gust", "wind_shield", "tempest"]),
    (
        "hero-husk",
        ["grasping_wraps", "dread", "lash_out", "tomb_bind"],
    ),
    (
        "hero-kensho",
        ["flicker_strike", "still_mind", "honed_edge", "unbound"],
    ),
    (
        "hero-rime",
        ["chill_arrows", "fan_of_frost", "snow_owl", "glacier_arrow"],
    ),
    (
        "hero-veil",
        ["dusk_mark", "smoke_ring", "whirling_blades", "night_step"],
    ),
];

/// A hero's weapon's action slot, whose attacks the showcase does not count as casts.
const WEAPON: u8 = 6;
/// A hero's first action slot of its inventory, after its four abilities, its two spells and its
/// weapon.
const INVENTORY: u8 = 7;

/// A hero as a tick left it.
#[derive(Debug, Clone)]
struct HeroState {
    level: u32,
    points: u32,
    slots: Vec<ActionSlot>,
    pools: Pools,
    carried: Vec<Option<Carried>>,
    modifiers: Vec<ModifierId>,
    gold: i64,
}

/// What the match left after each tick from the one before the shop: each hero, by player slot.
#[derive(Debug, Default)]
pub(crate) struct Showcase {
    ticks: Vec<[HeroState; 6]>,
}

impl Showcase {
    /// The first tick it reads.
    const FIRST: u64 = Reference3v3::SHOP - 1;

    /// Reads tick `tick` of the 3v3 of `reference`, which `world` just ran.
    pub(crate) fn read(&mut self, world: &World, reference: &Reference3v3, tick: u64) {
        if tick < Showcase::FIRST {
            return;
        }
        let units = MatchUnits::of_world(world);
        let heroes = [0, 1, 2, 3, 4, 5].map(|slot| {
            let hero = units.hero(slot);
            let entity = world.resource::<EntityIndex>().get(hero).unwrap();
            let unit = world.entity(entity);
            HeroState {
                level: unit.get::<Level>().unwrap().get(),
                points: unit.get::<Points>().unwrap().get(),
                slots: unit.get::<ActionSlots>().unwrap().iter().collect(),
                pools: *unit.get::<Pools>().unwrap(),
                carried: unit.get::<Inventory>().unwrap().slots().to_vec(),
                modifiers: internals::carried(world, hero)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect(),
                gold: gold(world, reference, slot),
            }
        });
        self.ticks.push(heroes);
    }

    /// Player `player`'s hero after tick `tick`.
    fn at(&self, tick: u64, player: u32) -> &HeroState {
        let at = usize::try_from(tick - Showcase::FIRST).unwrap();
        &self.ticks[at][usize::try_from(player).unwrap()]
    }
}

/// The tick the showcase's `index`th cast is sent in.
const fn stamp(index: u64) -> u64 {
    Reference3v3::SHOWCASE + Reference3v3::CAST_EVERY * index
}

/// The item `name` of the 3v3.
fn item(reference: &Reference3v3, name: &str) -> ItemId {
    let mode = reference.packages().packages().next().unwrap();
    ItemId::named(&mode.content.items, name).unwrap()
}

/// The farm: every hero reaches level 6 and learns every ability, its ultimate among them, with
/// a point each, so its points left are its level less its ranks.
pub(crate) fn assert_farm(showcase: &Showcase) {
    for player in 0..Reference3v3::PLAYERS {
        let hero = showcase.at(Reference3v3::SHOWCASE - 1, player);
        let ranks: Vec<u8> = hero.slots[..4].iter().map(|slot| slot.rank).collect();
        assert!(hero.level >= 6, "player {player}: level {}", hero.level);
        assert!(
            ranks.iter().all(|&rank| rank >= 1),
            "player {player}: {ranks:?}"
        );
        let spent: u32 = ranks.iter().map(|&rank| u32::from(rank)).sum();
        assert_eq!(hero.points, hero.level - spent, "player {player}");
    }
}

/// The shop, in the tick after an income: each hero pays each item's cost, or a built item's
/// cost less its components', and Veil sells her long sword back for 70% of its cost in the
/// next tick; each item lands in the first empty slot, a built one in its first component's.
pub(crate) fn assert_shop(reference: &Reference3v3, showcase: &Showcase) {
    let shop = Reference3v3::SHOP;
    let paid =
        |player, tick: u64| showcase.at(tick - 1, player).gold - showcase.at(tick, player).gold;
    // Husk: a cloth armor, 300, a ruby crystal, 400, then the stoneplate they build, 1000 less
    // 700. Kensho: a cloth armor, then the quicksilver it builds, 900 less 300. Gale: two wards
    // of 75. Rime: a health potion, 50. Cinder: a mana potion, 50, and an elixir, 250. Veil: a
    // long sword, 350, and a health potion, 50.
    let costs: Vec<i64> = (0..Reference3v3::PLAYERS)
        .map(|player| paid(player, shop))
        .collect();
    assert_eq!(costs, [300, 150, 1000, 900, 50, 400]);
    // 350 × 0.7 = 245, back to Veil in the next tick.
    assert_eq!(paid(5, shop + 1), -245);

    let carried = |player| -> Vec<Option<ItemId>> {
        let hero = showcase.at(shop + 1, player);
        hero.carried
            .iter()
            .map(|slot| slot.map(|carried| carried.item))
            .collect()
    };
    let one = |name| Some(item(reference, name));
    let empty = |taken: Vec<Option<ItemId>>| {
        let mut slots = taken;
        slots.resize(6, None);
        slots
    };
    let expected = [
        empty(vec![one("mana_potion"), one("elixir_of_sight")]),
        empty(vec![one("sight_ward"), one("vision_ward")]),
        empty(vec![one("stoneplate")]),
        empty(vec![one("quicksilver")]),
        empty(vec![one("health_potion")]),
        // The long sword went to slot 0 and the potion to slot 1; the swap put the potion first.
        empty(vec![one("health_potion")]),
    ];
    let seen: Vec<_> = (0..Reference3v3::PLAYERS).map(carried).collect();
    assert_eq!(seen, expected);
}

/// The data of player `player`'s action in slot `slot`: an ability of its hero's package, or the
/// active of the item it carries there, of the mode's.
fn action(reference: &Reference3v3, player: u32, slot: u8) -> &ActionData {
    let (package, name) = match (player, slot) {
        (2, INVENTORY) => ("moba-3v3", "harden"),
        (3, INVENTORY) => ("moba-3v3", "cleanse"),
        _ => {
            let (package, abilities) = ABILITIES[usize::try_from(player).unwrap()];
            (package, abilities[usize::from(slot)])
        }
    };
    let view = reference
        .packages()
        .packages()
        .find(|view| view.package.header.name == package);
    let name = DeclaredName::new(name).unwrap();
    &view.unwrap().content.actions[&name]
}

/// A whole number of `ranked` at `rank`.
fn int(ranked: &Ranked<Number>, rank: u8) -> i64 {
    let Some(&Number::Value(Scalar::Int(value))) = ranked.get(rank) else {
        panic!("a whole number at rank {rank}");
    };
    value
}

/// What player `player`'s toggles that pay each second paid of the pool `name` in tick `at`:
/// each one's cost at its rank, when it was due in that tick.
fn toggles_paid(
    reference: &Reference3v3,
    showcase: &Showcase,
    player: u32,
    at: u64,
    name: &DeclaredName,
) -> Num {
    let slots = &showcase.at(at - 1, player).slots[..4];
    let due = (0..)
        .zip(slots)
        .filter(|(_, slot)| slot.toggle == Some(Tick::new(at)));
    due.filter_map(|(index, slot)| {
        let Some(Toggle::CostPerSecond(costs)) = &action(reference, player, index).toggle else {
            return None;
        };
        Some(Num::int(int(costs.get(name)?, slot.rank)))
    })
    .fold(Num::ZERO, |sum, cost| sum + cost)
}

/// A start the showcase saw: player `player`'s action in slot `slot` began its cooldown, spent a
/// charge or turned its toggle on, in tick `tick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Start {
    tick: u64,
    player: u32,
    slot: u8,
}

impl Showcase {
    /// Every start from the showcase's first tick, in the order of the ticks, then of the players
    /// and the slots.
    fn starts(&self, end: u64) -> Vec<Start> {
        let mut starts = Vec::new();
        for tick in Reference3v3::SHOWCASE..end {
            for player in 0..Reference3v3::PLAYERS {
                let before = &self.at(tick - 1, player).slots;
                let after = &self.at(tick, player).slots;
                for (slot, (was, is)) in (0..).zip(before.iter().zip(after)) {
                    if slot == WEAPON {
                        continue;
                    }
                    let cooled = is.ready_at > Tick::new(tick) && is.ready_at > was.ready_at;
                    let charged = matches!(
                        (was.charges, is.charges),
                        (Some(was), Some(is)) if is.count < was.count
                    ) || (was.charges.is_none() && is.charges.is_some());
                    let toggled = was.toggle.is_none() && is.toggle.is_some();
                    if cooled || charged || toggled {
                        starts.push(Start { tick, player, slot });
                    }
                }
            }
        }
        starts
    }
}

/// The showcase's casts: each starts its cooldown as its windup ends after the tick it is sent
/// in, or, for one whose target stands out of its range, after its hero has walked into it; each
/// pays its cost at its rank, and starts
/// its cooldown, spends a charge or turns its toggle on as its data says. The consumables are
/// used up, the potions and the elixir give their modifiers, the wards stand where Gale placed
/// them, and the toggles turn off when their heroes cast them again.
pub(crate) fn assert_casts(reference: &Reference3v3, showcase: &Showcase, world: &World, end: u64) {
    // Each start, by the index of its cast in the showcase, its player, its slot, and whether its
    // hero walks into range first: the item actives in slot 7, and the abilities in slots 0 to 3.
    // Cyclone, Gale's first, charges as its first cast starts and starts its cooldown as the
    // second releases it; the toggles, Dread and Chill Arrows, start as they turn on; Night Step
    // spends a charge. Kensho walks to Cinder for Flicker Strike, and Cinder to Veil for
    // Wildfire.
    let expected = [
        (0, 2, INVENTORY, false),
        (7, 5, 0, false),
        (8, 1, 1, false),
        (9, 3, INVENTORY, false),
        (10, 5, 1, false),
        (11, 1, 2, false),
        (12, 5, 2, false),
        (14, 1, 0, false),
        (15, 5, 3, false),
        (16, 1, 3, false),
        (17, 0, 0, false),
        (18, 2, 1, false),
        (19, 4, 0, false),
        (21, 3, 0, true),
        (23, 0, 1, false),
        (24, 2, 2, false),
        (25, 4, 1, false),
        (26, 3, 1, false),
        (27, 0, 2, true),
        (30, 2, 0, false),
        (31, 4, 2, false),
        (32, 3, 2, false),
        (33, 2, 3, false),
        (34, 0, 3, false),
        (35, 4, 3, false),
        (36, 3, 3, false),
    ];
    let starts = showcase.starts(end);
    let seen: Vec<(u32, u8)> = starts
        .iter()
        .map(|start| (start.player, start.slot))
        .collect();
    let wanted: Vec<(u32, u8)> = expected
        .iter()
        .map(|&(_, player, slot, _)| (player, slot))
        .collect();
    assert_eq!(seen, wanted);
    let rate = *world.resource::<TickRate>();
    let ticks = |ms: i64| rate.ticks(u64::try_from(ms).unwrap()).unwrap().get();
    for (start, &(index, .., walks)) in starts.iter().zip(&expected) {
        let Start { tick, player, slot } = *start;
        let data = action(reference, player, slot);
        let is = &showcase.at(tick, player).slots[usize::from(slot)];
        let rank = is.rank;
        let windup = data
            .windup_ms
            .as_ref()
            .map_or(0, |windup| ticks(int(windup, rank)));
        let goes_off = stamp(index) + windup;
        if walks {
            assert!(
                (goes_off + 1..stamp(index + 2)).contains(&tick),
                "{start:?}"
            );
        } else {
            assert_eq!(tick, goes_off, "{start:?}");
        }
        if let Some(cooldown) = &data.cooldown_ms {
            assert_eq!(
                is.ready_at,
                Tick::new(tick + ticks(int(cooldown, rank))),
                "{start:?}"
            );
        }
        if let Some(charges) = &data.charges {
            let charges_now = is.charges.unwrap();
            let max = u8::try_from(int(&charges.max, rank)).unwrap();
            let recharge = ticks(int(&charges.recharge_ms, rank));
            assert_eq!(charges_now.count, max - 1, "{start:?}");
            assert_eq!(charges_now.next, Tick::new(tick + recharge), "{start:?}");
        }
        if data.toggle.is_some() {
            assert!(is.toggle.is_some(), "{start:?}");
        }
        // The cost, beside the pool's regeneration, taken as the tick before's, and the toggles'
        // payments of each second: a tick adds the bits of its second's regeneration divided by
        // the ticks a second, and carries the remainder to the next, so it adds as much as the
        // tick before, or a bit more or less.
        let pools = reference.packages().data();
        for (name, cost) in &data.cost {
            let pool = PoolId::named(&pools.pools, name).unwrap();
            let left = |at| showcase.at(at, player).pools.current(pool).unwrap();
            let paid = |at| toggles_paid(reference, showcase, player, at, name);
            let regen = left(tick - 1) - left(tick - 2) + paid(tick - 1);
            let spent = left(tick - 1) + regen - left(tick) - paid(tick);
            let off = spent - Num::int(int(cost, rank));
            assert!(off.to_bits().abs() <= 1, "{start:?}: spent {spent}");
        }
    }

    // The toggles turn off as Husk and Rime cast them again, in casts 28 and 29.
    let toggle = |tick, player, slot: usize| showcase.at(tick, player).slots[slot].toggle;
    assert!(toggle(stamp(28) - 1, 2, 1).is_some() && toggle(stamp(28), 2, 1).is_none());
    assert!(toggle(stamp(29) - 1, 4, 0).is_some() && toggle(stamp(29), 4, 0).is_none());

    assert_consumables(reference, showcase, world);
}

/// The consumables: casts 1 to 3 and 6 drink a potion or the elixir, each the only one its hero
/// carries in its slot, which empties as it starts, and give its modifier; casts 4 and 5 place
/// Gale's wards 3 m and a little more from her, which spend her two slots: the sight ward lives
/// 90 s, 1800 ticks, and the vision ward with no end.
fn assert_consumables(reference: &Reference3v3, showcase: &Showcase, world: &World) {
    let modifier = |name| Stats::modifier(world, 0, name).unwrap();
    let drinks = [
        (1, 4, 0, "health_potion"),
        (2, 5, 0, "health_potion"),
        (3, 0, 0, "mana_potion"),
        (6, 0, 1, "elixir_of_sight"),
        (4, 1, 0, "sight_ward"),
        (5, 1, 1, "vision_ward"),
    ];
    for (index, player, inventory, name) in drinks {
        let tick = stamp(index);
        let before = showcase.at(tick - 1, player);
        let after = showcase.at(tick, player);
        let carried = before.carried[inventory].unwrap();
        assert_eq!(carried.item, item(reference, name), "cast {index}");
        assert_eq!(after.carried[inventory], None, "cast {index}");
        if !name.ends_with("ward") {
            let id = modifier(name);
            assert!(
                !before.modifiers.contains(&id) && after.modifiers.contains(&id),
                "{name}"
            );
        }
    }
    let north = Team::new(0);
    let ward = |x: i64| {
        let at = Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(-23))).unwrap();
        let index = world.resource::<EntityIndex>();
        let found = index
            .iter()
            .find(|&(_, entity)| world.get::<Position>(entity) == Some(&at));
        let (_, entity) = found.expect("a ward");
        let team = *world.get::<Team>(entity).unwrap();
        (team, world.get::<Lifespan>(entity).map(|life| life.ends()))
    };
    let sight_ends = Tick::new(stamp(4) + 1800);
    assert_eq!(ward(0), (north, Some(sight_ends)));
    assert_eq!(ward(1), (north, None));
}
