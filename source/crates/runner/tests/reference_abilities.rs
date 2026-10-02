//! The reference heroes' abilities as their packages hold them: every ability's data reads into
//! the typed schema, and Husk's Lash Out, Kensho's Twin Cut, Veil's Dusk Mark and Smoke Ring,
//! Rime's Fan of Frost and Snow Owl, and Cinder's Eruption, loaded as a match of the 3v3 loads
//! them, act exactly. The tests pin the content's values, so a change to the content changes them,
//! by design.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::Path;

use bevy_ecs::bundle::Bundle;

use campfire_capabilities::internals::{self, Arms};
use campfire_capabilities::{
    Action, ActionId, ActionSlots, ActionTarget, Area, DeclaredName, Hook, ModifierId, Modifiers,
    Number, OnDeath, Order, Owner, Param, Pools, Projectile, Range, RangeField, Ranked,
    RecentAttackers, Scalar, Scaling, SlotKind, Stat, Targeting, Team,
};
use campfire_content::PackagePath;
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_package::{AvatarData, PackageDir, PackageFiles};
use campfire_runner::internals::Arena;
use campfire_sim::{EntityIndex, IdAllocator, Position, StableId, TickRate};

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const MODE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba/modes/3v3");

const HEROES: [&str; 6] = ["cinder", "gale", "husk", "kensho", "rime", "veil"];

fn hero(name: &str) -> PackageFiles {
    let dir = PackageDir::new(format!(
        "{}/../../packages/moba/heroes/{name}",
        env!("CARGO_MANIFEST_DIR")
    ));
    dir.read().unwrap()
}

/// The hero `name`, as its data file declares it.
fn abilities(name: &str) -> AvatarData {
    let path = PackagePath::parse("data/avatar.toml").unwrap();
    hero(name).read_data(&path).unwrap()
}
/// The 3v3's books at 30 ticks a second for one player, with no mode: no `calc_damage` weighs a
/// hit, so each one lands as its ability deals it.
fn arena() -> Arena {
    Arena::new(Path::new(MODE), RATE, 1)
}

/// A unit of 500 health on `team` at `x` meters along x that stays when it dies, with `parts`, and
/// with `mana` and `energy` pools of those maxima, when not 0; with no slots unless `parts` hold
/// some.
fn spawn_with(
    arena: &mut Arena,
    team: u8,
    x: i64,
    pools: (i64, i64),
    parts: impl Bundle,
) -> StableId {
    let at = Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::ZERO)).unwrap();
    spawn_at(arena, team, at, pools, parts)
}

/// A unit as `spawn_with` gives, at `at`.
fn spawn_at(
    arena: &mut Arena,
    team: u8,
    at: Position,
    (mana, energy): (i64, i64),
    parts: impl Bundle,
) -> StableId {
    let pools = [("health", 500), ("mana", mana), ("energy", energy)]
        .into_iter()
        .filter(|&(_, max)| max > 0)
        .map(|(pool, max)| (arena.pool(pool), Num::int(max)));
    let pools = Pools::new(pools).unwrap();
    let world = arena.world_mut();
    let id = world.resource_mut::<IdAllocator>().allocate();
    let combat = (OnDeath::Stay, RecentAttackers::default());
    let mut unit = world.spawn((id, at, Team::new(team), pools, combat, parts));
    unit.insert_if_new(ActionSlots::new([]));
    id
}

/// A unit of 500 health and no other pool, as `spawn_with` gives.
fn spawn(arena: &mut Arena, team: u8, x: i64, parts: impl Bundle) -> StableId {
    spawn_with(arena, team, x, (0, 0), parts)
}

fn health(arena: &Arena, id: StableId) -> i64 {
    pool(arena, id, "health").round()
}

fn pool(arena: &Arena, id: StableId, name: &str) -> Num {
    let world = arena.world();
    let entity = world.resource::<EntityIndex>().get(id).unwrap();
    let pools = world.get::<Pools>(entity).unwrap();
    pools.current(arena.pool(name)).unwrap()
}

/// Gives `unit` a weapon of `damage` within 2 m, its windup of no ticks, every `period` ticks.
fn arm(arena: &mut Arena, unit: StableId, damage: i64, period: u64) {
    let world = arena.world_mut();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    let arms = Arms::melee(Num::int(2), 0, period, Num::int(damage));
    let parts = arms.parts(world);
    world.entity_mut(entity).insert(parts);
}

const fn attack(unit: StableId, target: StableId) -> Order {
    Order {
        unit,
        action: Action::Attack { target },
    }
}

/// Player 0's order that `unit` casts the action in its slot 0 at `target`.
const fn cast(unit: StableId, target: ActionTarget) -> Order {
    Order {
        unit,
        action: Action::Slot { slot: 0, target },
    }
}

/// Player 0's unit with `action` at `rank` in its one slot, and `mana` and `energy`.
fn caster(
    arena: &mut Arena,
    action: ActionId,
    rank: u8,
    pools: (i64, i64),
    parts: impl Bundle,
) -> StableId {
    let slots = ActionSlots::new([(action, SlotKind::new(0), rank)]);
    let player = Owner::new(PlayerSlot::new(0));
    spawn_with(arena, 0, 0, pools, (player, slots, parts))
}

/// The modifiers `unit` carries, each with its source.
fn carried(arena: &Arena, unit: StableId) -> Vec<(ModifierId, Option<StableId>)> {
    internals::carried(arena.world(), unit)
}

/// How many projectiles the match holds.
fn projectiles(arena: &mut Arena) -> usize {
    let world = arena.world_mut();
    world.query::<&Projectile>().iter(world).count()
}

/// How many areas the match holds.
fn areas(arena: &mut Arena) -> usize {
    let world = arena.world_mut();
    world.query::<&Area>().iter(world).count()
}

/// The point `x`, `z` thousandths of a meter from the origin.
fn milli(x: i64, z: i64) -> Position {
    let at = |value: i64| Num::int(value).checked_div_int(1000).unwrap();
    Position::new(Vec3::new(at(x), Num::ZERO, at(z))).unwrap()
}

#[test]
fn every_reference_ability_reads_into_the_schema() {
    let mut read = 0;
    for name in HEROES {
        let data = abilities(name);
        for (ability, data) in &data.content.actions {
            if let Some(script) = &data.script {
                assert!(hero(name).read_text(script).is_ok(), "{name}.{ability}");
            }
            read += 1;
        }
    }
    // Four abilities and a weapon a hero.
    assert_eq!(read, 30);

    // Husk's Lash Out reads exactly as its file writes it.
    let husk = abilities("husk");
    let lash_out = &husk.content.actions["lash_out"];
    assert_eq!(lash_out.targeting, Targeting::None);
    assert_eq!(lash_out.range, None);
    let int = |value| Number::Value(Scalar::Int(value));
    let cooldowns = [10_000, 9000, 8000, 7000, 6000].map(int).to_vec();
    assert_eq!(lash_out.cooldown_ms, Some(Ranked::PerRank(cooldowns)));
    let mana = DeclaredName::new("mana").unwrap();
    assert_eq!(
        lash_out.cost,
        BTreeMap::from([(mana, Ranked::One(int(35)))])
    );
    // "3.5" is 7 halves; "0.5" one half.
    let half = Num::HALF;
    assert_eq!(
        lash_out.params["radius"],
        Param::Ranked(Ranked::One(Scalar::Decimal(half * 7)))
    );
    let Param::Scaling(Scaling {
        base,
        bonus,
        ratios,
        ..
    }) = &lash_out.params["damage"]
    else {
        panic!("damage scales");
    };
    assert_eq!(base.at(2), Some(Scalar::Int(100)));
    let ability_power = Stat::named("ability_power").unwrap();
    assert_eq!(*ratios, [(ability_power, half)].into());
    assert!(bonus.is_empty());
    // Veil's spell vamp scales with bonus attack damage: 0.00167 × 2²⁴ = 28 017.95 bits, to
    // 28 018.
    let veil = abilities("veil");
    let Param::Scaling(Scaling { bonus, ratios, .. }) =
        &veil.content.modifiers["dual_path"].params["spell_vamp"]
    else {
        panic!("spell vamp scales");
    };
    let attack_damage = Stat::named("attack_damage").unwrap();
    let ratio = Num::from_bits(28_018);
    assert_eq!(*bonus, [(attack_damage, ratio)].into());
    assert!(ratios.is_empty());
    // Rime's Snow Owl reaches farther at each rank.
    let rime = abilities("rime");
    let Some(Ranked::PerRank(ranges)) = &rime.content.actions["snow_owl"].range else {
        panic!("a range per rank");
    };
    assert_eq!(ranges[1], RangeField::Range(Range::Meters(half * 65)));
    let wraps = &husk.content.actions["grasping_wraps"];
    assert_eq!(wraps.targeting, Targeting::Direction);
    let eleven = RangeField::Range(Range::Meters(Num::int(11)));
    assert_eq!(wraps.range, Some(Ranked::One(eleven)));
}

#[test]
fn lash_out_from_its_package_hits_exactly() {
    let mut arena = arena();
    let lash_out = arena.action("hero-husk", "lash_out");
    let caster = caster(&mut arena, lash_out, 3, (100, 0), ());
    let near = spawn(&mut arena, 1, 3, ());
    let far = spawn(&mut arena, 1, 4, ());
    arena.tick(0, &[cast(caster, ActionTarget::None)]);
    // Rank 3 deals 125 within 3.5 m: 500 → 375 at 3 m, and nothing at 4 m.
    assert_eq!(health(&arena, near), 375);
    assert_eq!(health(&arena, far), 500);
}

#[test]
fn kenshos_twin_cut_hits_twice_on_each_seventh_attack_and_never_answers_itself() {
    let mut arena = arena();
    let player = Owner::new(PlayerSlot::new(0));
    let blade = spawn(&mut arena, 0, 0, (player, Modifiers::default()));
    let dummy = spawn(&mut arena, 1, 1, ());
    let twin_cut = arena.modifier("hero-kensho", "twin_cut");
    internals::give_modifier(
        arena.world_mut(),
        blade,
        twin_cut,
        Some((blade, None, 1)),
        true,
    );
    // 20 an attack, every 10 ticks from tick 0, so 7 within 4 s are consecutive: 6 by tick 50,
    // 500 − 120; the 7th, in tick 60, hits twice, − 40; its extra hit counts for nothing, so
    // the next 6 take 120 by tick 120, and the 14th, in tick 130, hits twice, − 40.
    arm(&mut arena, blade, 20, 10);
    arena.tick(0, &[attack(blade, dummy)]);
    let mut healths = vec![health(&arena, dummy)];
    for _ in 1..=130 {
        arena.step();
        healths.push(health(&arena, dummy));
    }
    let at = |tick: usize| healths[tick];
    assert_eq!([at(50), at(60), at(120), at(130)], [380, 340, 220, 180]);
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn veils_dusk_mark_detonates_once_on_veils_next_damage() {
    let mut arena = arena();
    let ability = arena.action("hero-veil", "dusk_mark");
    let dusk_mark = arena.modifier("hero-veil", "dusk_mark");
    let player = Owner::new(PlayerSlot::new(0));
    let veil = spawn_with(&mut arena, 0, 0, (0, 200), player);
    let energy = arena.pool("energy");
    let world = arena.world_mut();
    let entity = world.resource::<EntityIndex>().get(veil).unwrap();
    let pools = *world.get::<Pools>(entity).unwrap();
    world
        .entity_mut(entity)
        .insert(internals::spent(pools, energy, Num::int(100)));
    let other = spawn(&mut arena, 0, 0, player);
    let marked = spawn(&mut arena, 1, 1, Modifiers::default());
    let marks = |arena: &Arena| !carried(arena, marked).is_empty();
    let from = Some((veil, Some(ability), 2));
    internals::give_modifier(arena.world_mut(), marked, dusk_mark, from, false);
    arm(&mut arena, other, 30, 100);
    arm(&mut arena, veil, 30, 100);
    // Another unit's attack of 30 in tick 0 leaves the mark.
    arena.tick(0, &[attack(other, marked)]);
    assert_eq!((health(&arena, marked), marks(&arena)), (470, true));
    // Veil's attack of 30 in tick 1 detonates it at rank 2: 70 more, and 25 energy back, from 100
    // to 125. The detonation is Dusk Mark's own damage, and the mark is gone: it detonates once.
    arena.tick(0, &[attack(veil, marked)]);
    assert_eq!((health(&arena, marked), marks(&arena)), (370, false));
    assert_eq!(pool(&arena, veil, "energy"), Num::int(125));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn rimes_fan_of_frost_from_its_package_hits_exactly_the_units_in_reach_once_each() {
    let mut arena = arena();
    // Fan of Frost runs no script: its data's `on_hit` deals its damage and applies its slow.
    assert_eq!(
        abilities("rime").content.actions["fan_of_frost"].script,
        None
    );
    let fan = arena.action("hero-rime", "fan_of_frost");
    let caster = caster(&mut arena, fan, 1, (100, 0), ());
    // Seven arrows from the origin, 57.5° apart from the first to the last, around +x: at 0°,
    // ±9.583°, ±19.167° and ±28.75°, each 0.4 m wide, so a body of no radius within 0.2 m of an
    // arrow's path is in reach, up to the 12 m of the range.
    let enemy = |arena: &mut Arena, at| spawn_at(arena, 1, at, (0, 0), Modifiers::default());
    // 1 m out on +x, 1 · sin 9.583° = 0.166 m from the arrows beside the middle one: three
    // arrows reach it in the same tick, and the cast hits it once.
    let near = enemy(&mut arena, milli(1000, 0));
    // On +x at 10 m: the middle arrow, which the near one did not stop, as another arrow of its
    // cast hit that one first.
    let far = enemy(&mut arena, milli(10_000, 0));
    // 7.998 m out at ±28.774°, 0.003 m from the outer arrows.
    let outer = [3850, -3850].map(|z| enemy(&mut arena, milli(7010, z)));
    // 12.5 m out at ±19.167°, past the range: 0.5 m from where the arrows end.
    let beyond = [4104, -4104].map(|z| enemy(&mut arena, milli(11_807, z)));
    // 6 m out at ±4.78°, between two arrows: 0.50 m from each.
    let between = [500, -500].map(|z| enemy(&mut arena, milli(5980, z)));
    let ally = spawn_at(&mut arena, 0, milli(5000, 0), (0, 0), Modifiers::default());

    arena.tick(0, &[cast(caster, ActionTarget::Point(milli(10_000, 0)))]);
    // The cast resolves after its windup of 8 ticks, and the arrows fly 0.5 m a tick for 24.
    for _ in 0..40 {
        arena.step();
    }
    assert_eq!(projectiles(&mut arena), 0);

    // Rank 1 deals 40, and slows.
    let slowed = |unit: StableId| !carried(&arena, unit).is_empty();
    let hit = [near, far, outer[0], outer[1]];
    let missed = [beyond[0], beyond[1], between[0], between[1], ally];
    assert_eq!(hit.map(|unit| health(&arena, unit)), [460; 4]);
    assert_eq!(missed.map(|unit| health(&arena, unit)), [500; 5]);
    assert_eq!(hit.map(slowed), [true; 4]);
    assert_eq!(missed.map(slowed), [false; 5]);
    assert_eq!(pool(&arena, caster, "mana"), Num::int(40));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn rimes_snow_owl_flies_to_its_point_and_ends_there() {
    let mut arena = arena();
    let snow_owl = arena.action("hero-rime", "snow_owl");
    let caster = caster(&mut arena, snow_owl, 1, (0, 0), ());
    let owls = |arena: &mut Arena| -> Vec<Position> {
        let world = arena.world_mut();
        let mut owls = world.query::<(&Projectile, &Position)>();
        owls.iter(world).map(|(_, &at)| at).collect()
    };

    // Aimed at a point 7 m out, within its 25 m range: it launches in tick 0, and flies 14/30 m
    // a tick, 7 829 367 bits rounded, from tick 1. Fifteen steps are 7 bits short of 7 m, and the
    // sixteenth ends it on the point, where its `on_end` runs.
    let point = Position::new(Vec3::new(Num::int(7), Num::ZERO, Num::ZERO)).unwrap();
    arena.tick(0, &[cast(caster, ActionTarget::Point(point))]);
    for _ in 1..=15 {
        arena.step();
    }
    let short = Num::from_bits(15 * 7_829_367);
    assert_eq!(short, Num::int(7) - Num::from_bits(7));
    let flying = Position::new(Vec3::new(short, Num::ZERO, Num::ZERO)).unwrap();
    assert_eq!(owls(&mut arena), [flying]);
    arena.step();
    assert_eq!(owls(&mut arena), []);
    // Its `on_end` ran, for its caster, and failed only on `ctx.reveal`, which vision plans.
    let failures = arena.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(
        (failures[0].unit, failures[0].hook),
        (Some(caster), Hook::OnEnd)
    );
}

#[test]
fn cinders_eruption_from_its_package_erupts_on_the_units_in_reach_after_its_delay() {
    let mut arena = arena();
    let eruption = arena.action("hero-cinder", "eruption");
    let kindle = arena.modifier("hero-cinder", "kindle");
    let caster = caster(&mut arena, eruption, 1, (100, 0), ());
    // The area lands on (6, 0, 0), of radius 2.4, and these bodies have no radius: one on the
    // centre and one 2.4 m out are in reach, one 2.401 m out is not, and an ally is never hit.
    let enemy = |arena: &mut Arena, at| spawn_at(arena, 1, at, (0, 0), Modifiers::default());
    let center = enemy(&mut arena, milli(6000, 0));
    let edge = enemy(&mut arena, milli(8400, 0));
    let beyond = enemy(&mut arena, milli(6000, 2401));
    let burning = enemy(&mut arena, milli(6000, 1000));
    let ally = spawn_at(&mut arena, 0, milli(6000, 0), (0, 0), Modifiers::default());
    let from = Some((caster, Some(eruption), 1));
    internals::give_modifier(arena.world_mut(), burning, kindle, from, false);

    // The cast starts in tick 0 and resolves after its windup of 250 ms, 8 ticks at 30 a
    // second, in tick 8, where the area lands; it erupts 625 ms later, 19 ticks rounded up, in
    // tick 27.
    arena.tick(0, &[cast(caster, ActionTarget::Point(milli(6000, 0)))]);
    let units = [center, edge, beyond, burning, ally];
    for _ in 1..27 {
        arena.step();
    }
    assert_eq!(units.map(|unit| health(&arena, unit)), [500; 5]);
    assert_eq!(areas(&mut arena), 1);
    arena.step();
    // Rank 1 deals 75, and 1.25 × 75 = 93.75 to a unit ablaze: 500 → 406.25. Each unit hit is
    // ablaze after, and the area ends with its eruption.
    assert_eq!(
        units.map(|unit| health(&arena, unit)),
        [425, 425, 500, 406, 500]
    );
    let ablaze = |unit| carried(&arena, unit) == [(kindle, Some(caster))];
    assert_eq!(units.map(ablaze), [true, true, false, true, false]);
    assert_eq!(areas(&mut arena), 0);
    assert_eq!(pool(&arena, caster, "mana"), Num::int(30));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn veils_smoke_ring_from_its_package_holds_its_modifiers_on_the_units_inside_while_it_lasts() {
    let mut arena = arena();
    assert_eq!(abilities("veil").content.actions["smoke_ring"].script, None);
    let ring = arena.action("hero-veil", "smoke_ring");
    let cover = arena.modifier("hero-veil", "smoke_ring_cover");
    let slow = arena.modifier("hero-veil", "smoke_ring_slow");
    let caster = caster(&mut arena, ring, 1, (0, 100), Modifiers::default());
    let unit =
        |arena: &mut Arena, team, x| spawn_with(arena, team, x, (0, 0), Modifiers::default());
    // The ring lands where Veil stands, of radius 3: an enemy 2 m and one 3 m away are inside,
    // one 4 m away is not, and an ally inside gets nothing, as the ring names no `allies`.
    let near = unit(&mut arena, 1, 2);
    let edge = unit(&mut arena, 1, 3);
    let far = unit(&mut arena, 1, 4);
    let ally = unit(&mut arena, 0, 1);
    let held = |arena: &Arena| [caster, near, edge, far, ally].map(|unit| carried(arena, unit));
    let from_veil = |id| vec![(id, Some(caster))];
    let inside = [
        from_veil(cover),
        from_veil(slow),
        from_veil(slow),
        vec![],
        vec![],
    ];

    // The cast resolves at once and the ring lands in tick 0; it holds its modifiers from that
    // tick's Resolve. It lasts 8000 ms, 240 ticks, and ends in tick 240.
    arena.tick(0, &[cast(caster, ActionTarget::None)]);
    assert_eq!(held(&arena), inside);
    assert_eq!(pool(&arena, caster, "energy"), Num::int(20));
    // The near enemy steps out to 4 m: its slow ends in the next tick's Resolve.
    for _ in 1..100 {
        arena.step();
    }
    let world = arena.world_mut();
    let entity = world.resource::<EntityIndex>().get(near).unwrap();
    *world.get_mut::<Position>(entity).unwrap() =
        Position::new(Vec3::new(Num::int(4), Num::ZERO, Num::ZERO)).unwrap();
    arena.step();
    let mut left = inside.clone();
    left[1] = vec![];
    assert_eq!(held(&arena), left);
    for _ in 101..240 {
        arena.step();
    }
    assert_eq!(held(&arena), left);
    assert_eq!(areas(&mut arena), 1);
    arena.step();
    assert_eq!(held(&arena), [vec![], vec![], vec![], vec![], vec![]]);
    assert_eq!(areas(&mut arena), 0);
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}
