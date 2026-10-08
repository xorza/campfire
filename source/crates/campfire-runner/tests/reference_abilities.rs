//! The reference heroes' abilities as their packages hold them: every ability's data reads into
//! the typed schema, and Husk's Lash Out and Grasping Wraps, Kensho's Twin Cut, Veil's Dusk Mark,
//! Smoke Ring and Night Step, Rime's Fan of Frost and Snow Owl, Cinder's Eruption and Chain Fire,
//! and the Farsight spell,
//! loaded as a match of the 3v3 loads them, act exactly. The tests pin the content's values, so a
//! change to the content changes them, by design.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bevy_ecs::bundle::Bundle;

use campfire_capabilities::internals::{self, Arms};
use campfire_capabilities::{
    Action, ActionId, ActionRange, ActionSlots, ActionTarget, Area, Body, DeclaredName, ForcedMove,
    ModifierId, Modifiers, MoveStep, Navigation, Number, OnDeath, Order, Owner, PackagePath, Param,
    Pools, Projectile, RangeField, Rank, Ranked, RecentAttackers, Scalar, Scaling, SeenBy,
    SlotKind, Stat, Targeting, Team,
};
use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_package::{AvatarData, PackageDir, PackageFiles};
use campfire_runner::internals::Arena;
use campfire_sim::{EntityIndex, IdAllocator, Position, StableId, TickRate};

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const HEROES: [&str; 6] = ["cinder", "gale", "husk", "kensho", "rime", "veil"];

fn hero(name: &str) -> PackageFiles {
    let dir = PackageDir::new(PackageDir::workspace(&format!("moba/heroes/{name}")));
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
    arena_at(RATE)
}

/// The same at `rate`.
fn arena_at(rate: TickRate) -> Arena {
    Arena::new(&PackageDir::workspace("moba/modes/3v3"), rate, 1)
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
    Order::one(unit, Action::Attack { target })
}

/// Player 0's order that `unit` casts the action in its slot 0 at `target`.
const fn cast(unit: StableId, target: ActionTarget) -> Order {
    Order::one(unit, Action::Slot { slot: 0, target })
}

/// Player 0's unit with `action` at `rank` in its one slot, and `mana` and `energy`.
fn caster(
    arena: &mut Arena,
    action: ActionId,
    rank: u8,
    pools: (i64, i64),
    parts: impl Bundle,
) -> StableId {
    let slots = ActionSlots::new([(action, SlotKind::new(0), Rank::new(rank))]);
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
    assert_eq!(base.get(Rank::new(2).unwrap()), Some(&Num::int(100)));
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
    assert_eq!(ranges[1], RangeField::Range(ActionRange::Meters(half * 65)));
    let wraps = &husk.content.actions["grasping_wraps"];
    assert_eq!(wraps.targeting, Targeting::Direction);
    let eleven = RangeField::Range(ActionRange::Meters(Num::int(11)));
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

/// Arms `unit` as `arm` does, with `ability` at rank 1 in slot 0, before the weapon.
fn arm_with(arena: &mut Arena, unit: StableId, ability: ActionId, damage: i64, period: u64) {
    arm(arena, unit, damage, period);
    let world = arena.world_mut();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    let weapon = world.get::<ActionSlots>(entity).unwrap().slot(0).unwrap();
    let slots = ActionSlots::new([
        (ability, SlotKind::new(0), Some(Rank::FIRST)),
        (weapon.action.unwrap(), weapon.kind, weapon.rank),
    ]);
    world.entity_mut(entity).insert(slots);
}

/// Whether `unit` has the toggle of its slot 0 on.
fn toggled(arena: &Arena, unit: StableId) -> bool {
    let world = arena.world();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    let slots = world.get::<ActionSlots>(entity).unwrap();
    slots.slot(0).unwrap().toggle.is_some()
}

#[test]
fn rimes_chill_arrows_pays_mana_each_attack_and_turns_off_when_it_cannot() {
    let mut arena = arena();
    let chill = arena.action("hero-rime", "chill_arrows");
    let hold = arena.modifier("hero-rime", "chill_arrows");
    let player = Owner::new(PlayerSlot::new(0));
    // Rime with 20 mana, a weapon of 10 every 10 ticks; an enemy beside her.
    let rime = spawn_with(&mut arena, 0, 0, (20, 0), (player, Modifiers::default()));
    let enemy = spawn(&mut arena, 1, 1, Modifiers::default());
    arm_with(&mut arena, rime, chill, 10, 10);
    let mana = |arena: &Arena| pool(arena, rime, "mana").round();
    let holds = |arena: &Arena| carried(arena, rime).contains(&(hold, Some(rime)));
    // Tick 0: on, at no cost but its toggle's, and its hold from that tick's Resolve.
    arena.tick(0, &[cast(rime, ActionTarget::None)]);
    assert_eq!(
        (toggled(&arena, rime), holds(&arena), mana(&arena)),
        (true, true, 20)
    );
    // Each attack pays 8 as it goes off, in ticks 1 and 11: 4 are left. The third, in tick 21,
    // cannot pay: the toggle turns off before it, and its hold with it.
    arena.tick(0, &[attack(rime, enemy)]);
    for _ in 2..=11 {
        arena.step();
    }
    assert_eq!((toggled(&arena, rime), mana(&arena)), (true, 4));
    for _ in 12..=21 {
        arena.step();
    }
    assert_eq!(
        (toggled(&arena, rime), holds(&arena), mana(&arena)),
        (false, false, 4)
    );
    assert_eq!(health(&arena, enemy), 470);
    // On again, then a second cast turns it off as it starts, at no cost.
    arena.tick(0, &[cast(rime, ActionTarget::None)]);
    assert!(toggled(&arena, rime));
    arena.tick(0, &[cast(rime, ActionTarget::None)]);
    assert_eq!((toggled(&arena, rime), mana(&arena)), (false, 4));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn husks_dread_pays_mana_each_second_burns_enemies_near_and_ends_at_death() {
    let mut arena = arena();
    let dread = arena.action("hero-husk", "dread");
    // Husk with 20 mana; an enemy of 500 health 2 m away, inside Dread's 3 m.
    let husk = caster(&mut arena, dread, 1, (20, 0), Modifiers::default());
    let enemy = spawn(&mut arena, 1, 2, ());
    let mana = |arena: &Arena| pool(arena, husk, "mana").round();
    // On in tick 0. It pays 8 at each whole second, in ticks 30 and 60: 4 are left. Its hold's
    // interval of 1000 ms deals 1.5% of the enemy's 500 health in ticks 30 and 60. In tick 90 it
    // cannot pay: it turns off before its hold's interval, which deals nothing more.
    arena.tick(0, &[cast(husk, ActionTarget::None)]);
    let mut seen = Vec::new();
    for tick in 1..=95 {
        arena.step();
        if [30, 60, 90].contains(&tick) {
            seen.push((
                tick,
                toggled(&arena, husk),
                mana(&arena),
                pool(&arena, enemy, "health"),
            ));
        }
    }
    // 0.015 in 24 fraction bits is 251 658, its 0.24 rounded off, so each burn is 500 times
    // that: a hair below 7.5.
    let burn = Num::int(500) * Num::from_bits(251_658);
    let after = |burns: i64| Num::int(500) - burn * Num::int(burns);
    assert_eq!(
        seen,
        [
            (30, true, 12, after(1)),
            (60, true, 4, after(2)),
            (90, false, 4, after(2)),
        ]
    );
    // On again, it ends as Husk dies.
    arena.tick(0, &[cast(husk, ActionTarget::None)]);
    assert!(toggled(&arena, husk));
    internals::queue_damage(arena.world_mut(), None, husk, Num::int(1000), "true");
    arena.step();
    arena.step();
    assert!(!toggled(&arena, husk));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn kenshos_still_mind_heals_each_half_second_of_its_channel_and_guards_him_while_it_runs() {
    let mut arena = arena();
    let still_mind = arena.action("hero-kensho", "still_mind");
    let guard = arena.modifier("hero-kensho", "still_mind");
    // Kensho with 100 mana, and 300 of his 500 health lost.
    let kensho = caster(&mut arena, still_mind, 1, (100, 0), Modifiers::default());
    let health = arena.pool("health");
    let world = arena.world_mut();
    let entity = world.resource::<EntityIndex>().get(kensho).unwrap();
    let pools = *world.get::<Pools>(entity).unwrap();
    world
        .entity_mut(entity)
        .insert(internals::spent(pools, health, Num::int(300)));
    let guarded = |arena: &Arena| carried(arena, kensho).contains(&(guard, Some(kensho)));
    // Cast in tick 0, for 50 mana. The channel runs from tick 1 for 5000 ms, 150 ticks, and
    // ticks each 500 ms, 15 ticks: in ticks 16 to 151, ten times, each 20 at rank 1 with no
    // ability power. It guards him from tick 0's Resolve until it ends in tick 151.
    arena.tick(0, &[cast(kensho, ActionTarget::None)]);
    assert_eq!(
        (pool(&arena, kensho, "mana"), guarded(&arena)),
        (Num::int(50), true)
    );
    let mut healed = Vec::new();
    for tick in 1..=152 {
        arena.step();
        if [15, 16, 150, 151].contains(&tick) {
            healed.push((tick, pool(&arena, kensho, "health"), guarded(&arena)));
        }
    }
    let at = |left: i64| Num::int(left);
    assert_eq!(
        healed,
        [
            (15, at(200), true),
            (16, at(220), true),
            (150, at(380), true),
            (151, at(400), false),
        ]
    );
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
fn rimes_snow_owl_flies_to_its_point_and_ends_there_and_its_sight_lingers() {
    let mut arena = arena();
    arena.load_vision(2);
    let snow_owl = arena.action("hero-rime", "snow_owl");
    let caster = caster(&mut arena, snow_owl, 1, (0, 0), ());
    // An enemy at (10, 0): its cell's center (10.5, 0.5) is √12.5 ≈ 3.54 m from the point, within
    // the owl's sight of 4 m and its linger's radius of 4 m; the caster has no sight of its own.
    let enemy = spawn(&mut arena, 1, 10, ());
    let seen = |arena: &Arena| {
        let world = arena.world();
        let entity = world.resource::<EntityIndex>().get(enemy).unwrap();
        world
            .get::<SeenBy>(entity)
            .unwrap()
            .get()
            .contains(Team::new(0))
    };
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
    assert!(!seen(&arena));
    for _ in 1..=15 {
        arena.step();
    }
    let short = Num::from_bits(15 * 7_829_367);
    assert_eq!(short, Num::int(7) - Num::from_bits(7));
    let flying = Position::new(Vec3::new(short, Num::ZERO, Num::ZERO)).unwrap();
    assert_eq!(owls(&mut arena), [flying]);
    arena.step();
    assert_eq!(owls(&mut arena), []);
    // Its `on_end` in tick 16 reveals 4 m round the point for 5000 ms, 150 ticks: the Vision
    // stages of ticks 16 to 165, with the owl gone.
    assert!(seen(&arena));
    for _ in 17..=165 {
        arena.step();
    }
    assert!(seen(&arena));
    arena.step();
    assert!(!seen(&arena));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn farsight_shows_its_cells_to_the_caster_team_alone_for_its_time() {
    let mut arena = arena();
    arena.load_vision(3);
    let farsight = arena.action("player-spells", "farsight");
    let caster = caster(&mut arena, farsight, 1, (0, 0), ());
    // Revealed 6 m round (20, 0): the cell of (25, 0), its center (25.5, 0.5) √30.5 ≈ 5.52 m
    // off, and not that of (26, 0), at (26.5, 0.5) √42.5 ≈ 6.52 m off. Team 2 sees neither.
    let near = spawn(&mut arena, 1, 25, ());
    let far = spawn(&mut arena, 1, 26, ());
    let seen = |arena: &Arena, unit: StableId| {
        let world = arena.world();
        let entity = world.resource::<EntityIndex>().get(unit).unwrap();
        let teams = world.get::<SeenBy>(entity).unwrap().get();
        [0, 2].map(|team| teams.contains(Team::new(team)))
    };
    // It resolves at once, in tick 0, and reveals for 5000 ms, 150 ticks: ticks 0 to 149.
    let point = Position::new(Vec3::new(Num::int(20), Num::ZERO, Num::ZERO)).unwrap();
    arena.tick(0, &[cast(caster, ActionTarget::Point(point))]);
    assert_eq!(
        [near, far].map(|unit| seen(&arena, unit)),
        [[true, false], [false; 2]]
    );
    for _ in 1..=149 {
        arena.step();
    }
    assert_eq!(seen(&arena, near), [true, false]);
    arena.step();
    assert_eq!(seen(&arena, near), [false; 2]);
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn cinders_eruption_from_its_package_erupts_on_the_units_in_reach_after_its_delay() {
    // The cast starts in tick 0 and resolves after its windup of 250 ms, where the area lands;
    // it erupts 625 ms later, each rounded up to whole ticks. At 30 a second: 7.5 ticks to 8, and
    // 18.75 to 19, in tick 27. At 20, the 3v3's rate: 5 ticks, and 12.5 to 13, in tick 18.
    for (hz, erupts) in [(30, 27), (20, 18)] {
        erupt_at(TickRate::new(NonZeroU32::new(hz).unwrap()), erupts);
    }
}

/// Cinder's Eruption at `rate`, which erupts in tick `erupts`.
fn erupt_at(rate: TickRate, erupts: u64) {
    let mut arena = arena_at(rate);
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

    arena.tick(0, &[cast(caster, ActionTarget::Point(milli(6000, 0)))]);
    let units = [center, edge, beyond, burning, ally];
    for _ in 1..erupts {
        arena.step();
    }
    assert_eq!(units.map(|unit| health(&arena, unit)), [500; 5], "{rate:?}");
    assert_eq!(areas(&mut arena), 1);
    arena.step();
    // Rank 1 deals 75, and 1.25 × 75 = 93.75 to a unit ablaze: 500 → 406.25. Each unit hit is
    // ablaze after, and the area ends with its eruption.
    assert_eq!(
        units.map(|unit| health(&arena, unit)),
        [425, 425, 500, 406, 500],
        "{rate:?}"
    );
    let ablaze = |unit| carried(&arena, unit) == [(kindle, Some(caster))];
    assert_eq!(units.map(ablaze), [true, true, false, true, false]);
    assert_eq!(areas(&mut arena), 0);
    assert_eq!(pool(&arena, caster, "mana"), Num::int(30));
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn cinders_chain_fire_bounces_between_two_enemies_until_its_bounces_run_out() {
    let mut arena = arena();
    let chain_fire = arena.action("hero-cinder", "chain_fire");
    let caster = caster(&mut arena, chain_fire, 1, (100, 0), ());
    // Two enemies 4 m apart, within the 5 m of a bounce. The arena has no map, so its Vision
    // stage never runs: each is seen by every team, as a bounce picks among what its caster's
    // team sees.
    let near = spawn(&mut arena, 1, 4, internals::seen_by_all());
    let far = spawn(&mut arena, 1, 8, internals::seen_by_all());
    let projectiles = |arena: &mut Arena| -> Vec<u64> {
        let world = arena.world_mut();
        let mut query = world.query::<(&Projectile, &StableId)>();
        query.iter(world).map(|(_, id)| id.get()).collect()
    };

    // The cast's projectile, id 3 after the caster's 0 and the enemies' 1 and 2, flies at 10 m a
    // second, a third of a meter a tick rounded down, so 4 m takes 13 steps: it hits `near` in
    // tick 13. Each hit deals 150 at rank 1 and, with its type's 4 bounces left and counting
    // down, picks the one other enemy in reach: 4 bounces after the first hit, each 4 m and 13
    // ticks, alternating, ids 4 to 7. The last hit, with none left, bounces no more.
    arena.tick(0, &[cast(caster, ActionTarget::Unit(near))]);
    let mut flown = Vec::new();
    let mut hits = Vec::new();
    let mut last = [500, 500];
    for tick in 1..=80 {
        flown.extend(projectiles(&mut arena));
        arena.step();
        let now = [near, far].map(|unit| health(&arena, unit));
        if now != last {
            hits.push((tick, now));
            last = now;
        }
    }
    flown.dedup();
    assert_eq!(flown, [3, 4, 5, 6, 7]);
    assert_eq!(projectiles(&mut arena), Vec::<u64>::new());
    assert_eq!(
        hits,
        [
            (13, [350, 500]),
            (26, [350, 350]),
            (39, [200, 350]),
            (52, [200, 200]),
            (65, [50, 200]),
        ]
    );
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

/// A body of half a meter.
fn body() -> Body {
    Body::new(Num::ONE.checked_div_int(2).unwrap()).unwrap()
}

/// A body of half a meter that walks a meter a tick.
fn walker() -> impl Bundle {
    let step = MoveStep::new(Num::ONE).unwrap();
    (body(), Navigation::walker(step))
}

/// Where `unit` stands along x, and the forced move under way on it.
fn forced(arena: &Arena, unit: StableId) -> (Num, Option<ForcedMove>) {
    let world = arena.world();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    let x = world.get::<Position>(entity).unwrap().get().x;
    (x, world.get::<ForcedMove>(entity).copied())
}

#[test]
fn veils_night_step_dashes_to_its_target_and_strikes_as_the_dash_ends_beside_it() {
    let mut arena = arena();
    let night_step = arena.action("hero-veil", "night_step");
    let veil = caster(&mut arena, night_step, 1, (0, 200), walker());
    let enemy = spawn(&mut arena, 1, 6, body());

    // It resolves at once in tick 0, and its dash of 22 m a second goes 22/30 m a tick, s, from
    // tick 1, until the bodies of half a meter touch, 1 m apart. Six steps leave 6 − 6s = 1.6 m,
    // within 1 + s; the seventh, in tick 7, ends the dash on 5 m, and its `on_end` deals rank 1's
    // 100 magic, with no ability power, in that tick.
    arena.tick(0, &[cast(veil, ActionTarget::Unit(enemy))]);
    for _ in 1..=6 {
        arena.step();
    }
    let (x, dash) = forced(&arena, veil);
    let step = Num::int(22).checked_div_int(30).unwrap();
    assert_eq!((x, health(&arena, enemy)), (step * 6, 500));
    assert!(matches!(
        dash,
        Some(ForcedMove::Dash {
            delivers: Some(_),
            ..
        })
    ));
    arena.step();
    assert_eq!(forced(&arena, veil), (Num::int(5), None));
    assert_eq!(health(&arena, enemy), 400);
    for _ in 0..30 {
        arena.step();
    }
    assert_eq!(health(&arena, enemy), 400);
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}

#[test]
fn husks_grasping_wraps_pulls_him_to_the_enemy_hit_and_its_pull_delivers_nothing() {
    let mut arena = arena();
    let wraps = arena.action("hero-husk", "grasping_wraps");
    let husk = caster(&mut arena, wraps, 1, (200, 0), walker());
    let enemy = spawn(&mut arena, 1, 5, (body(), Modifiers::default()));

    // The cast resolves after its windup of 250 ms, 8 ticks, and its wraps fly 20/30 m a tick
    // from tick 9, 1.6 m wide: they reach the enemy's body once their front comes within 0.8 +
    // 0.5 m of its center, in their sixth step, tick 14. Its `on_hit` deals rank 1's 80, stuns,
    // and pulls Husk at 18 m a second: a dash from the delivery's hit, not from `on_resolve`, so
    // it delivers no action and its end runs no `on_end`.
    arena.tick(0, &[cast(husk, ActionTarget::Point(milli(5000, 0)))]);
    let mut pulled = None;
    for tick in 1..=40 {
        arena.step();
        if pulled.is_none() && forced(&arena, husk).1.is_some() {
            pulled = Some((tick, forced(&arena, husk).1.unwrap()));
        }
    }
    let (tick, pull) = pulled.unwrap();
    assert_eq!(tick, 14);
    assert!(matches!(pull, ForcedMove::Dash { delivers: None, .. }));
    // The pull ends with the bodies touching, Husk on 4 m, and the enemy took the hit alone.
    assert_eq!(forced(&arena, husk), (Num::int(4), None));
    assert_eq!(health(&arena, enemy), 420);
    assert!(arena.failures().is_empty(), "{:?}", arena.failures());
}
