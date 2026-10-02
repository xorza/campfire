//! The reference heroes' abilities as their packages hold them: every ability's data reads into
//! the typed schema, and Husk's Lash Out, Kensho's Twin Cut, Veil's Dusk Mark and Smoke Ring,
//! Rime's Fan of Frost and Cinder's Eruption, loaded from their data files and scripts, act
//! exactly.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::World;

use campfire_capabilities::internals::{self, Arms};
use campfire_capabilities::{
    Abilities, Action, ActionSlots, ActionTarget, Actions, Area, Areas, CapabilitySet,
    DeclaredName, Hook, MatchScripts, Number, OnDeath, Order, Owner, Param, PoolId, Pools,
    Projectile, Projectiles, Range, RangeField, Ranked, RecentAttackers, Scalar, Scaling,
    ScriptLimits, SlotKind, Stat, StatRule, Stats, Targeting, Team, Units,
};
use campfire_capabilities::{Modifiers, ScriptFailure, ScriptFailures};
use campfire_content::PackagePath;
use campfire_math::{Num, PlayerSlot, SegmentSeed, Vec3};
use campfire_package::{AvatarData, PackageDir, PackageFiles};
use campfire_script::ScriptId;
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, Position, SimUpdate, StableId, StateRegistry, TickInput,
    TickInputs, TickRate,
};

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

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

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// The pools of the match, by name in order: the life pool first, as it is until a mode binds
/// one.
const POOLS: [&str; 3] = ["health", "mana", "energy"];
const MANA: PoolId = PoolId::new(1).unwrap();
const ENERGY: PoolId = PoolId::new(2).unwrap();

/// A unit of 500 health on `team` at `x` meters along x that stays when it dies, with `parts`, and
/// with `mana` and `energy` pools of those maxima, when not 0; with no slots unless `parts` hold
/// some.
fn spawn_with(
    world: &mut World,
    team: u8,
    x: i64,
    pools: (i64, i64),
    parts: impl Bundle,
) -> StableId {
    let at = Position::new(Vec3::new(num(x), Num::ZERO, Num::ZERO)).unwrap();
    spawn_at(world, team, at, pools, parts)
}

/// A unit as `spawn_with` gives, at `at`.
fn spawn_at(
    world: &mut World,
    team: u8,
    at: Position,
    (mana, energy): (i64, i64),
    parts: impl Bundle,
) -> StableId {
    let id = world.resource_mut::<IdAllocator>().allocate();
    let pools = [(PoolId::FIRST, 500), (MANA, mana), (ENERGY, energy)]
        .into_iter()
        .filter(|&(_, max)| max > 0)
        .map(|(pool, max)| (pool, num(max)));
    let combat = (OnDeath::Stay, RecentAttackers::default());
    let pools = Pools::new(pools).unwrap();
    let mut unit = world.spawn((id, at, Team::new(team), pools, combat, parts));
    unit.insert_if_new(ActionSlots::new([]));
    id
}

/// A unit of 500 health and no other pool, as `spawn_with` gives.
fn spawn(world: &mut World, team: u8, x: i64, parts: impl Bundle) -> StableId {
    spawn_with(world, team, x, (0, 0), parts)
}

#[test]
fn every_reference_ability_reads_into_the_schema() {
    let mut read = 0;
    for name in HEROES {
        let data = abilities(name);
        for (ability, data) in &data.actions {
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
    let lash_out = &husk.actions["lash_out"];
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
    let half = Num::from_bits(1 << 23);
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
    assert_eq!(*ratios, [(ability_power, Scalar::Decimal(half))].into());
    assert!(bonus.is_empty());
    // Veil's spell vamp scales with bonus attack damage: 0.00167 × 2²⁴ = 28 017.95 bits, to
    // 28 018.
    let veil = abilities("veil");
    let Param::Scaling(Scaling { bonus, ratios, .. }) =
        &veil.modifiers["dual_path"].params["spell_vamp"]
    else {
        panic!("spell vamp scales");
    };
    let attack_damage = Stat::named("attack_damage").unwrap();
    let ratio = Scalar::Decimal(Num::from_bits(28_018));
    assert_eq!(*bonus, [(attack_damage, ratio)].into());
    assert!(ratios.is_empty());
    // Rime's Snow Owl reaches farther at each rank.
    let rime = abilities("rime");
    let Some(Ranked::PerRank(ranges)) = &rime.actions["snow_owl"].range else {
        panic!("a range per rank");
    };
    assert_eq!(ranges[1], RangeField::Range(Range::Meters(half * 65)));
    let wraps = &husk.actions["grasping_wraps"];
    assert_eq!(wraps.targeting, Targeting::Direction);
    let eleven = RangeField::Range(Range::Meters(num(11)));
    assert_eq!(wraps.range, Some(Ranked::One(eleven)));
}

/// A match of 1 player with the capabilities the reference abilities use, the reference MOBA's
/// damage kinds, and a stat book of the stats the tested modifiers change.
fn reference_world() -> World {
    let mut world = World::new();
    SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), RATE);
    let mut schedule = SimUpdate::schedule();
    let mut registry = StateRegistry::new();
    let limits = ScriptLimits {
        per_call: 10_000,
        player: 100_000,
        think: 100_000,
        mode: 100_000,
    };
    let kinds = ["physical", "magic", "true"].map(|kind| DeclaredName::new(kind).unwrap());
    // The stats the heroes' params name, in the order the mode's stats hold them.
    let mut stats =
        ["ability_power", "attack_damage", "spell_vamp"].map(|name| Stat::named(name).unwrap());
    stats.sort();
    let scripts = MatchScripts {
        limits,
        players: 1,
        damage_kinds: Rc::from(kinds),
        stats: stats.into(),
        pools: POOLS.map(|pool| DeclaredName::new(pool).unwrap()).into(),
        resources: Rc::from([]),
    };
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
        Capability::Projectiles,
        Capability::Areas,
    ];
    let set = CapabilitySet::new(&declared).unwrap();
    set.install(&mut world, &mut schedule, &mut registry, Some(scripts));
    world.add_schedule(schedule);
    world.insert_non_send(Failed::default());
    let changed = ["move_speed", "armor", "magic_resist"]
        .map(|name| (Stat::named(name).unwrap(), StatRule::default()));
    internals::load_stats(&mut world, &BTreeMap::from(changed), RATE);
    world
}

/// The script at `path` of the hero `name`, compiled.
fn compile(world: &mut World, name: &str, path: &PackagePath) -> ScriptId {
    let hero = hero(name);
    Units::compile(world, hero.read_text(path).unwrap()).unwrap()
}

/// Runs a tick in which player 0 orders each of `orders`.
fn tick(world: &mut World, orders: &[Order]) {
    let payload = Order::payload(orders);
    world.resource_mut::<TickInputs>().push(TickInput {
        slot: PlayerSlot::new(0),
        payload: &payload,
    });
    step(world);
}

/// Every script failure of the ticks `step` ran, in order. `ScriptFailures` keeps only the last
/// tick's.
#[derive(Debug, Default)]
struct Failed(Vec<ScriptFailure>);

/// Runs one tick, and keeps its script failures.
fn step(world: &mut World) {
    world.run_schedule(SimUpdate);
    let failures = world.non_send::<ScriptFailures>().get().to_vec();
    world.non_send_mut::<Failed>().0.extend(failures);
}

/// Every script failure since the match began.
fn failures(world: &World) -> &[ScriptFailure] {
    &world.non_send::<Failed>().0
}

#[test]
fn lash_out_from_its_package_hits_exactly() {
    let mut world = reference_world();
    let husk = abilities("husk");
    for (name, modifier) in &husk.modifiers {
        Stats::load_modifier(&mut world, 0, name, modifier, None);
    }
    let data = &husk.actions["lash_out"];
    let script = compile(&mut world, "husk", data.script.as_ref().unwrap());
    let lash_out = Actions::load(&mut world, 0, "lash_out", data, Some(script), 5).unwrap();

    let caster = spawn_with(
        &mut world,
        0,
        0,
        (100, 0),
        (
            Owner::new(PlayerSlot::new(0)),
            ActionSlots::new([(lash_out, SlotKind::new(0), 3)]),
        ),
    );
    let near = spawn(&mut world, 1, 3, ());
    let far = spawn(&mut world, 1, 4, ());

    tick(
        &mut world,
        &[Order {
            unit: caster,
            action: Action::Slot {
                slot: 0,
                target: ActionTarget::None,
            },
        }],
    );

    // Rank 3 deals 125 within 3.5 m: 500 → 375 at 3 m, and nothing at 4 m.
    assert_eq!(health(&world, near), 375);
    assert_eq!(health(&world, far), 500);
}

fn health(world: &World, id: StableId) -> i64 {
    pool(world, id, PoolId::FIRST).round()
}

fn pool(world: &World, id: StableId, pool: PoolId) -> Num {
    let entity = world.resource::<EntityIndex>().get(id).unwrap();
    world.get::<Pools>(entity).unwrap().current(pool).unwrap()
}

/// Gives `unit` a weapon of `damage` within 2 m, its windup of no ticks, every `period` ticks.
fn arm(world: &mut World, unit: StableId, damage: i64, period: u64) {
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    let arms = Arms::melee(num(2), 0, period, num(damage));
    let parts = arms.parts(world, RATE.hz().get());
    world.entity_mut(entity).insert(parts);
}

fn attack(unit: StableId, target: StableId) -> Order {
    Order {
        unit,
        action: Action::Attack { target },
    }
}

#[test]
fn kenshos_twin_cut_hits_twice_on_each_seventh_attack_and_never_answers_itself() {
    let mut world = reference_world();
    let kensho = abilities("kensho");
    let data = &kensho.modifiers["twin_cut"];
    let script = compile(&mut world, "kensho", data.script.as_ref().unwrap());
    Stats::load_modifier(&mut world, 0, "twin_cut", data, Some(script));
    let player = Owner::new(PlayerSlot::new(0));
    let blade = spawn(&mut world, 0, 0, (player, Modifiers::default()));
    let dummy = spawn(&mut world, 1, 1, ());
    let twin_cut = Stats::modifier(&world, 0, "twin_cut").unwrap();
    internals::give_modifier(&mut world, blade, twin_cut, Some((blade, None, 1)), true);
    // 20 an attack, every 10 ticks from tick 0, so 7 within 4 s are consecutive: 6 by tick 50,
    // 500 − 120; the 7th, in tick 60, hits twice, − 40; its extra hit counts for nothing, so
    // the next 6 take 120 by tick 120, and the 14th, in tick 130, hits twice, − 40.
    arm(&mut world, blade, 20, 10);
    tick(&mut world, &[attack(blade, dummy)]);
    let mut healths = vec![health(&world, dummy)];
    for _ in 1..=130 {
        step(&mut world);
        healths.push(health(&world, dummy));
    }
    let at = |tick: usize| healths[tick];
    assert_eq!([at(50), at(60), at(120), at(130)], [380, 340, 220, 180]);
    assert!(failures(&world).is_empty(), "{:?}", failures(&world));
}

#[test]
fn veils_dusk_mark_detonates_once_on_veils_next_damage() {
    let mut world = reference_world();
    let veil = abilities("veil");
    let mark = &veil.modifiers["dusk_mark"];
    let script = compile(&mut world, "veil", mark.script.as_ref().unwrap());
    Stats::load_modifier(&mut world, 0, "dusk_mark", mark, Some(script));
    let data = &veil.actions["dusk_mark"];
    let ability = Actions::load(&mut world, 0, "dusk_mark", data, Some(script), 5).unwrap();
    let player = Owner::new(PlayerSlot::new(0));
    let veil_unit = spawn_with(&mut world, 0, 0, (0, 200), player);
    let entity = world.resource::<EntityIndex>().get(veil_unit).unwrap();
    let pools = *world.get::<Pools>(entity).unwrap();
    world
        .entity_mut(entity)
        .insert(internals::spent(pools, ENERGY, num(100)));
    let other = spawn(&mut world, 0, 0, player);
    let marked = spawn(&mut world, 1, 1, Modifiers::default());
    let dusk_mark = Stats::modifier(&world, 0, "dusk_mark").unwrap();
    let marks = |world: &World| {
        let entity = world.resource::<EntityIndex>().get(marked).unwrap();
        *world.get::<Modifiers>(entity).unwrap() != Modifiers::default()
    };
    let from = Some((veil_unit, Some(ability), 2));
    internals::give_modifier(&mut world, marked, dusk_mark, from, false);
    arm(&mut world, other, 30, 100);
    arm(&mut world, veil_unit, 30, 100);
    // Another unit's attack of 30 in tick 0 leaves the mark.
    tick(&mut world, &[attack(other, marked)]);
    assert_eq!((health(&world, marked), marks(&world)), (470, true));
    // Veil's attack of 30 in tick 1 detonates it at rank 2: 70 more, and 25 energy back, from 100
    // to 125. The detonation is Dusk Mark's own damage, and the mark is gone: it detonates once.
    tick(&mut world, &[attack(veil_unit, marked)]);
    assert_eq!((health(&world, marked), marks(&world)), (370, false));
    assert_eq!(pool(&world, veil_unit, ENERGY), num(125));
    assert!(failures(&world).is_empty(), "{:?}", failures(&world));
}

#[test]
fn rimes_fan_of_frost_from_its_package_hits_exactly_the_units_in_reach_once_each() {
    let mut world = reference_world();
    let rime = abilities("rime");
    Stats::load_modifier(&mut world, 0, "slow", &rime.modifiers["slow"], None);
    let arrow = &rime.units["frost_arrow"];
    let frost_arrow = Units::load_type(&mut world, "rime/frost_arrow", &arrow.core).unwrap();
    Projectiles::load_type(&mut world, frost_arrow, arrow.projectile.as_ref().unwrap());
    // Fan of Frost runs no script: its data's `on_hit` deals its damage and applies its slow.
    let data = &rime.actions["fan_of_frost"];
    assert_eq!(data.script, None);
    let fan = Actions::load(&mut world, 0, "fan_of_frost", data, None, 5).unwrap();
    Abilities::load_effects(&mut world, fan, 0, data);
    Actions::bind_spawn(&mut world, fan, "rime/frost_arrow");

    let player = Owner::new(PlayerSlot::new(0));
    let slots = ActionSlots::new([(fan, SlotKind::new(0), 1)]);
    let caster = spawn_with(&mut world, 0, 0, (100, 0), (player, slots));
    let milli = |x: i64, z: i64| {
        let at = |value: i64| num(value).checked_div_int(1000).unwrap();
        Position::new(Vec3::new(at(x), Num::ZERO, at(z))).unwrap()
    };
    // Seven arrows from the origin, 57.5° apart from the first to the last, around +x: at 0°,
    // ±9.583°, ±19.167° and ±28.75°, each 0.4 m wide, so a body of no radius within 0.2 m of an
    // arrow's path is in reach, up to the 12 m of the range.
    let enemy = |world: &mut World, at| spawn_at(world, 1, at, (0, 0), Modifiers::default());
    // 1 m out on +x, 1 · sin 9.583° = 0.166 m from the arrows beside the middle one: three
    // arrows reach it in the same tick, and the cast hits it once.
    let near = enemy(&mut world, milli(1000, 0));
    // On +x at 10 m: the middle arrow, which the near one did not stop, as another arrow of its
    // cast hit that one first.
    let far = enemy(&mut world, milli(10_000, 0));
    // 7.998 m out at ±28.774°, 0.003 m from the outer arrows.
    let outer = [3850, -3850].map(|z| enemy(&mut world, milli(7010, z)));
    // 12.5 m out at ±19.167°, past the range: 0.5 m from where the arrows end.
    let beyond = [4104, -4104].map(|z| enemy(&mut world, milli(11_807, z)));
    // 6 m out at ±4.78°, between two arrows: 0.50 m from each.
    let between = [500, -500].map(|z| enemy(&mut world, milli(5980, z)));
    let ally = spawn_at(&mut world, 0, milli(5000, 0), (0, 0), Modifiers::default());

    tick(
        &mut world,
        &[Order {
            unit: caster,
            action: Action::Slot {
                slot: 0,
                target: ActionTarget::Point(milli(10_000, 0)),
            },
        }],
    );
    // The cast resolves after its windup of 8 ticks, and the arrows fly 0.5 m a tick for 24.
    for _ in 0..40 {
        step(&mut world);
    }
    let projectiles = world.resource::<EntityIndex>().iter();
    assert_eq!(
        projectiles
            .filter(|&(_, entity)| world.get::<Projectile>(entity).is_some())
            .count(),
        0
    );

    // Rank 1 deals 40, and slows.
    let slowed = |unit: StableId| {
        let entity = world.resource::<EntityIndex>().get(unit).unwrap();
        *world.get::<Modifiers>(entity).unwrap() != Modifiers::default()
    };
    let hit = [near, far, outer[0], outer[1]];
    let missed = [beyond[0], beyond[1], between[0], between[1], ally];
    assert_eq!(hit.map(|unit| health(&world, unit)), [460; 4]);
    assert_eq!(missed.map(|unit| health(&world, unit)), [500; 5]);
    assert_eq!(hit.map(slowed), [true; 4]);
    assert_eq!(missed.map(slowed), [false; 5]);
    assert_eq!(pool(&world, caster, MANA), num(40));
    assert!(failures(&world).is_empty(), "{:?}", failures(&world));
}

#[test]
fn rimes_snow_owl_flies_to_its_point_and_ends_there() {
    let mut world = reference_world();
    let rime = abilities("rime");
    let data = &rime.actions["snow_owl"];
    let script = compile(&mut world, "rime", data.script.as_ref().unwrap());
    let bounty = &rime.modifiers["snow_owl_bounty"];
    Stats::load_modifier(&mut world, 0, "snow_owl_bounty", bounty, Some(script));
    let owl = &rime.units["snow_owl"];
    let owl_type = Units::load_type(&mut world, "rime/snow_owl", &owl.core).unwrap();
    Projectiles::load_type(&mut world, owl_type, owl.projectile.as_ref().unwrap());
    let snow_owl = Actions::load(&mut world, 0, "snow_owl", data, Some(script), 5).unwrap();
    Actions::bind_spawn(&mut world, snow_owl, "rime/snow_owl");
    let player = Owner::new(PlayerSlot::new(0));
    let slots = ActionSlots::new([(snow_owl, SlotKind::new(0), 1)]);
    let caster = spawn(&mut world, 0, 0, (player, slots));
    let owls = |world: &World| -> Vec<Position> {
        let units = world.resource::<EntityIndex>().iter();
        units
            .filter(|&(_, entity)| world.get::<Projectile>(entity).is_some())
            .map(|(_, entity)| *world.get::<Position>(entity).unwrap())
            .collect()
    };

    // Aimed at a point 7 m out, within its 25 m range: it launches in tick 0, and flies 14/30 m
    // a tick, 7 829 367 bits rounded, from tick 1. Fifteen steps are 7 bits short of 7 m, and the
    // sixteenth ends it on the point, where its `on_end` runs.
    let point = Position::new(Vec3::new(num(7), Num::ZERO, Num::ZERO)).unwrap();
    tick(
        &mut world,
        &[Order {
            unit: caster,
            action: Action::Slot {
                slot: 0,
                target: ActionTarget::Point(point),
            },
        }],
    );
    for _ in 1..=15 {
        step(&mut world);
    }
    let short = Num::from_bits(15 * 7_829_367);
    assert_eq!(short, num(7) - Num::from_bits(7));
    let flying = Position::new(Vec3::new(short, Num::ZERO, Num::ZERO)).unwrap();
    assert_eq!(owls(&world), [flying]);
    step(&mut world);
    assert_eq!(owls(&world), []);
    // Its `on_end` ran, for its caster, and failed only on `ctx.reveal`, which vision plans.
    let failures = failures(&world);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(
        (failures[0].unit, failures[0].hook),
        (Some(caster), Hook::OnEnd)
    );
}

/// Loads every modifier of the hero `name`, with its script, as its package 0.
fn load_modifiers(world: &mut World, name: &str, data: &AvatarData) {
    for (id, modifier) in &data.modifiers {
        let script = modifier
            .script
            .as_ref()
            .map(|path| compile(world, name, path));
        Stats::load_modifier(world, 0, id, modifier, script);
    }
}

/// How many areas the match holds.
fn areas(world: &mut World) -> usize {
    world.query::<&Area>().iter(world).count()
}

#[test]
fn cinders_eruption_from_its_package_erupts_on_the_units_in_reach_after_its_delay() {
    let mut world = reference_world();
    let cinder = abilities("cinder");
    load_modifiers(&mut world, "cinder", &cinder);
    let file = &cinder.units["eruption"];
    let unit_type = Units::load_type(&mut world, "cinder/eruption", &file.core).unwrap();
    Areas::load_type(&mut world, unit_type, 0, file.area.as_ref().unwrap());
    let data = &cinder.actions["eruption"];
    let script = compile(&mut world, "cinder", data.script.as_ref().unwrap());
    let eruption = Actions::load(&mut world, 0, "eruption", data, Some(script), 5).unwrap();
    Actions::bind_spawn(&mut world, eruption, "cinder/eruption");

    let player = Owner::new(PlayerSlot::new(0));
    let slots = ActionSlots::new([(eruption, SlotKind::new(0), 1)]);
    let caster = spawn_with(&mut world, 0, 0, (100, 0), (player, slots));
    let milli = |x: i64, z: i64| {
        let at = |value: i64| num(value).checked_div_int(1000).unwrap();
        Position::new(Vec3::new(at(x), Num::ZERO, at(z))).unwrap()
    };
    // The area lands on (6, 0, 0), of radius 2.4, and these bodies have no radius: one on the
    // centre and one 2.4 m out are in reach, one 2.401 m out is not, and an ally is never hit.
    let enemy = |world: &mut World, at| spawn_at(world, 1, at, (0, 0), Modifiers::default());
    let center = enemy(&mut world, milli(6000, 0));
    let edge = enemy(&mut world, milli(8400, 0));
    let beyond = enemy(&mut world, milli(6000, 2401));
    let burning = enemy(&mut world, milli(6000, 1000));
    let ally = spawn_at(&mut world, 0, milli(6000, 0), (0, 0), Modifiers::default());
    let kindle = Stats::modifier(&world, 0, "kindle").unwrap();
    internals::give_modifier(
        &mut world,
        burning,
        kindle,
        Some((caster, Some(eruption), 1)),
        false,
    );

    // The cast starts in tick 0 and resolves after its windup of 250 ms, 8 ticks at 30 a
    // second, in tick 8, where the area lands; it erupts 625 ms later, 19 ticks rounded up, in
    // tick 27.
    tick(
        &mut world,
        &[Order {
            unit: caster,
            action: Action::Slot {
                slot: 0,
                target: ActionTarget::Point(milli(6000, 0)),
            },
        }],
    );
    let units = [center, edge, beyond, burning, ally];
    for _ in 1..27 {
        step(&mut world);
    }
    assert_eq!(units.map(|unit| health(&world, unit)), [500; 5]);
    assert_eq!(areas(&mut world), 1);
    step(&mut world);
    // Rank 1 deals 75, and 1.25 × 75 = 93.75 to a unit ablaze: 500 → 406.25. Each unit hit is
    // ablaze after, and the area ends with its eruption.
    assert_eq!(
        units.map(|unit| health(&world, unit)),
        [425, 425, 500, 406, 500]
    );
    let ablaze = |unit| internals::carried(&world, unit) == [(kindle, Some(caster))];
    assert_eq!(units.map(ablaze), [true, true, false, true, false]);
    assert_eq!(areas(&mut world), 0);
    assert_eq!(pool(&world, caster, MANA), num(30));
    assert!(failures(&world).is_empty(), "{:?}", failures(&world));
}

#[test]
fn veils_smoke_ring_from_its_package_holds_its_modifiers_on_the_units_inside_while_it_lasts() {
    let mut world = reference_world();
    let veil = abilities("veil");
    load_modifiers(&mut world, "veil", &veil);
    let file = &veil.units["smoke_ring"];
    let unit_type = Units::load_type(&mut world, "veil/smoke_ring", &file.core).unwrap();
    Areas::load_type(&mut world, unit_type, 0, file.area.as_ref().unwrap());
    let data = &veil.actions["smoke_ring"];
    assert_eq!(data.script, None);
    let ring = Actions::load(&mut world, 0, "smoke_ring", data, None, 5).unwrap();
    Actions::bind_spawn(&mut world, ring, "veil/smoke_ring");

    let player = Owner::new(PlayerSlot::new(0));
    let slots = ActionSlots::new([(ring, SlotKind::new(0), 1)]);
    let caster = spawn_with(
        &mut world,
        0,
        0,
        (0, 100),
        (player, slots, Modifiers::default()),
    );
    let unit =
        |world: &mut World, team, x| spawn_with(world, team, x, (0, 0), Modifiers::default());
    // The ring lands where Veil stands, of radius 3: an enemy 2 m and one 3 m away are inside,
    // one 4 m away is not, and an ally inside gets nothing, as the ring names no `allies`.
    let near = unit(&mut world, 1, 2);
    let edge = unit(&mut world, 1, 3);
    let far = unit(&mut world, 1, 4);
    let ally = unit(&mut world, 0, 1);
    let cover = Stats::modifier(&world, 0, "smoke_ring_cover").unwrap();
    let slow = Stats::modifier(&world, 0, "smoke_ring_slow").unwrap();
    let carried =
        |world: &World| [caster, near, edge, far, ally].map(|unit| internals::carried(world, unit));
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
    tick(
        &mut world,
        &[Order {
            unit: caster,
            action: Action::Slot {
                slot: 0,
                target: ActionTarget::None,
            },
        }],
    );
    assert_eq!(carried(&world), inside);
    assert_eq!(pool(&world, caster, ENERGY), num(20));
    // The near enemy steps out to 4 m: its slow ends in the next tick's Resolve.
    for _ in 1..100 {
        step(&mut world);
    }
    let entity = world.resource::<EntityIndex>().get(near).unwrap();
    *world.get_mut::<Position>(entity).unwrap() =
        Position::new(Vec3::new(num(4), Num::ZERO, Num::ZERO)).unwrap();
    step(&mut world);
    let mut left = inside.clone();
    left[1] = vec![];
    assert_eq!(carried(&world), left);
    for _ in 101..240 {
        step(&mut world);
    }
    assert_eq!(carried(&world), left);
    assert_eq!(areas(&mut world), 1);
    step(&mut world);
    assert_eq!(carried(&world), [vec![], vec![], vec![], vec![], vec![]]);
    assert_eq!(areas(&mut world), 0);
    assert!(failures(&world).is_empty(), "{:?}", failures(&world));
}
