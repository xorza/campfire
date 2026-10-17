//! The MOBA heroes' abilities as their packages hold them: every ability's data reads into
//! the typed schema, and Husk's Lash Out and Grasping Wraps, Kensho's Twin Cut, Veil's Dusk Mark,
//! Smoke Ring and Night Step, Rime's Fan of Frost and Snow Owl, Cinder's Eruption and Chain Fire,
//! and the Farsight spell,
//! loaded as a match of the 3v3 loads them, act exactly. The tests pin the content's values, so a
//! change to the content changes them, by design.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bevy_ecs::bundle::Bundle;
use campfire_actions::{
    ActionRange, ActionSlots, ActionTarget, RangeField, SlotKind, SlotTimes, Targeting, TimedSlots,
};
use campfire_areas::Area;
use campfire_capabilities::{Match, Order, PackageContent};
use campfire_combat::Combat;
use campfire_common::{DeclaredName, PlayerSlot};
use campfire_harness::Arena;
use campfire_math::{Num, Vec3};
use campfire_navigation::Navigation;
use campfire_orders::OrdersAction;
use campfire_package::{ModePackages, PackageFiles};
use campfire_package_disk::PackageDir;
use campfire_projectiles::Projectile;
use campfire_sim::{Position, SimWorld, StableId, TickRate};
use campfire_stats::{Life, PoolMeters, Pools, Stats};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{Body, ForcedMove, MoveStep, Owner};
use campfire_values::{
    ActionId, ModifierId, Number, Param, Rank, Ranked, Scalar, Scaling, Stat, Team,
};
use campfire_vision::SeenBy;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const HEROES: [&str; 6] = ["cinder", "gale", "husk", "kensho", "rime", "veil"];

fn hero(name: &str) -> PackageFiles {
    let dir = PackageDir::new(PackageDir::workspace(&format!("test/moba/heroes/{name}")));
    dir.files().unwrap()
}

/// The content of the hero `name`, as the mode of `packages` loads it.
fn abilities<'a>(packages: &'a ModePackages, name: &str) -> &'a PackageContent {
    let hero = format!("hero-{name}");
    let dependent = packages
        .dependencies()
        .iter()
        .find(|dependent| dependent.package.header.name.as_str() == hero)
        .expect("a hero of the mode");
    &dependent.content
}
/// The weapons of the tests' armed units: an attack within 2 m with no windup, at the rate and
/// for the damage of its unit's stats; and one as it is with a windup of 100 ms, 3 ticks, that
/// costs 12 mana.
const ACTIONS: &str = r#"
[actions.arena_strike]
kind = "attack"
targeting = "enemies"
range = "2.0"
windup_ms = 0
rate = "attack_speed"
damage = "attack_damage"
damage_kind = "physical"

[actions.arena_costly_strike]
kind = "attack"
targeting = "enemies"
range = "2.0"
windup_ms = 100
cost = { mana = 12 }
rate = "attack_speed"
damage = "attack_damage"
damage_kind = "physical"
"#;

/// The unit types the tests' units take, one for each kit: 500 health, a pool of mana or energy
/// of a maximum, and a weapon, each at level 1. Each stays when it dies, and has no tag a filter
/// reads. 2.5 attacks a second, the mode's most, is one every 30 / 2.5 = 12 ticks.
const UNITS: &str = r#"
[units.arena_bare]
pools = ["health"]
stats = { health = { base = 500 } }
combat = { on_death = "stay" }

[units.arena_mana_20]
pools = ["health", "mana"]
stats = { health = { base = 500 }, mana = { base = 20 } }
combat = { on_death = "stay" }

[units.arena_mana_100]
pools = ["health", "mana"]
stats = { health = { base = 500 }, mana = { base = 100 } }
combat = { on_death = "stay" }

[units.arena_mana_200]
pools = ["health", "mana"]
stats = { health = { base = 500 }, mana = { base = 200 } }
combat = { on_death = "stay" }

[units.arena_energy_100]
pools = ["health", "energy"]
stats = { health = { base = 500 }, energy = { base = 100 } }
combat = { on_death = "stay" }

[units.arena_energy_200]
pools = ["health", "energy"]
stats = { health = { base = 500 }, energy = { base = 200 } }
combat = { on_death = "stay" }

[units.arena_blade]
pools = ["health"]
stats = { health = { base = 500 }, attack_damage = { base = 20 }, attack_speed = { base = "2.5" } }
combat = { on_death = "stay" }
slots = { weapon = ["arena_strike"] }

[units.arena_archer]
pools = ["health", "mana"]
stats = { health = { base = 500 }, mana = { base = 20 }, attack_damage = { base = 10 }, attack_speed = { base = "2.5" } }
combat = { on_death = "stay" }
slots = { weapon = ["arena_strike"] }

[units.arena_costly_archer]
pools = ["health", "mana"]
stats = { health = { base = 500 }, mana = { base = 20 }, attack_damage = { base = 10 }, attack_speed = { base = "2.5" } }
combat = { on_death = "stay" }
slots = { weapon = ["arena_costly_strike"] }

[units.arena_striker]
pools = ["health"]
stats = { health = { base = 500 }, attack_damage = { base = 30 }, attack_speed = { base = 1 } }
combat = { on_death = "stay" }
slots = { weapon = ["arena_strike"] }

[units.arena_energy_striker]
pools = ["health", "energy"]
stats = { health = { base = 500 }, energy = { base = 200 }, attack_damage = { base = 30 }, attack_speed = { base = 1 } }
combat = { on_death = "stay" }
slots = { weapon = ["arena_strike"] }
"#;

/// A test's unit: 500 health, and no other pool or one of mana or energy of a maximum, and a
/// weapon of damage and attacks a second; each a unit type of `UNITS`.
#[derive(Debug, Clone, Copy)]
enum Kit {
    Bare,
    Mana20,
    Mana100,
    Mana200,
    Energy100,
    Energy200,
    /// 20 an attack, 2.5 a second.
    Blade,
    /// 20 mana; 10 an attack, 2.5 a second.
    Archer,
    /// As `Archer`, its attack of a 3-tick windup and 12 mana.
    CostlyArcher,
    /// 30 an attack, 1 a second.
    Striker,
    /// 200 energy; 30 an attack, 1 a second.
    EnergyStriker,
}

impl Kit {
    const fn unit_type(self) -> &'static str {
        match self {
            Self::Bare => "arena_bare",
            Self::Mana20 => "arena_mana_20",
            Self::Mana100 => "arena_mana_100",
            Self::Mana200 => "arena_mana_200",
            Self::Energy100 => "arena_energy_100",
            Self::Energy200 => "arena_energy_200",
            Self::Blade => "arena_blade",
            Self::Archer => "arena_archer",
            Self::CostlyArcher => "arena_costly_archer",
            Self::Striker => "arena_striker",
            Self::EnergyStriker => "arena_energy_striker",
        }
    }
}

/// The 3v3's books with no mode, and the actions and unit types of `ACTIONS` and `UNITS`: no
/// `calc_damage` weighs a hit, so each one lands as its ability deals it.
fn arena() -> Arena {
    let mut tree = PackageTree::of(TestMode::Moba3v3);
    tree.edit("modes/3v3/data/mode.toml", Edit::Append(ACTIONS))
        .edit("modes/3v3/data/units.toml", Edit::Append(UNITS));
    Arena::load(&tree)
}

/// A unit of `kit` on `team` at `x` meters along x, with `parts`.
fn spawn_with(
    arena: &Arena,
    game: &mut Match,
    x: i16,
    team: Team,
    kit: Kit,
    parts: impl Bundle,
) -> StableId {
    spawn_at(arena, game, (x, 0, 0), team, kit, parts)
}

/// A unit as `spawn_with` gives, at `pos`.
fn spawn_at(
    arena: &Arena,
    game: &mut Match,
    pos: impl Into<Vec3>,
    team: Team,
    kit: Kit,
    parts: impl Bundle,
) -> StableId {
    arena.spawn(game, kit.unit_type(), pos.into(), team, parts)
}

/// A unit of 500 health and no other pool, as `spawn_with` gives.
fn spawn(arena: &Arena, game: &mut Match, x: i16, team: Team, parts: impl Bundle) -> StableId {
    spawn_with(arena, game, x, team, Kit::Bare, parts)
}

fn health(arena: &Arena, game: &Match, id: StableId) -> i64 {
    pool(arena, game, id, "health").round()
}

fn pool(arena: &Arena, game: &Match, id: StableId, name: &str) -> Num {
    let entity = game.entity_of(id).unwrap();
    let meters = PoolMeters::of(game, entity);
    meters.current(arena.pool(name)).unwrap()
}

fn attack(id: StableId, target: StableId) -> Order {
    Order::one(id, OrdersAction::Attack { target })
}

/// Player 0's order that `id` casts the action in its slot 0 at `target`.
fn cast(id: StableId, target: ActionTarget) -> Order {
    Order::one(id, OrdersAction::Slot { slot: 0, target })
}

/// Player 0's unit of `kit` with `action` at `rank` in its one slot.
fn caster(
    arena: &Arena,
    game: &mut Match,
    action: ActionId,
    rank: u8,
    kit: Kit,
    parts: impl Bundle,
) -> StableId {
    let slots = TimedSlots::new(ActionSlots::new([(
        action,
        SlotKind::new(0),
        Rank::new(rank),
    )]));
    let player = Owner::new(PlayerSlot::new(0));
    spawn_with(arena, game, 0, Team::new(0), kit, (player, slots, parts))
}

/// The modifiers `id` carries, each with its source.
fn carried(game: &Match, id: StableId) -> Vec<(ModifierId, Option<StableId>)> {
    Stats::carried_modifiers(game, id)
}

/// How many projectiles the match holds.
fn projectiles(game: &mut Match) -> usize {
    game.query::<&Projectile>().iter(game).count()
}

/// How many areas the match holds.
fn areas(game: &mut Match) -> usize {
    game.query::<&Area>().iter(game).count()
}

/// The point `x`, `z` thousandths of a meter from the origin.
fn milli(x: i64, z: i64) -> Vec3 {
    let at = |value: i64| Num::int(value) / 1000;
    Vec3::new(at(x), Num::ZERO, at(z))
}

#[test]
fn every_moba_ability_reads_into_the_schema() {
    let packages = TestMode::Moba3v3.packages();
    let abilities = |name| abilities(&packages, name);
    let mut read = 0;
    for name in HEROES {
        let data = abilities(name);
        for (action, data) in data.actions() {
            if let Some(script) = &data.script {
                assert!(hero(name).read_text(script).is_ok(), "{name}.{action}");
            }
            read += 1;
        }
    }
    // Four abilities and a weapon a hero.
    assert_eq!(read, 30);

    // Husk's Lash Out reads exactly as its file writes it.
    let husk = abilities("husk");
    let lash_out = &husk.actions()["lash_out"];
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
        &veil.modifiers()["dual_path"].params["spell_vamp"]
    else {
        panic!("spell vamp scales");
    };
    let attack_damage = Stat::named("attack_damage").unwrap();
    let ratio = Num::from_bits(28_018);
    assert_eq!(*bonus, [(attack_damage, ratio)].into());
    assert!(ratios.is_empty());
    // Rime's Snow Owl reaches farther at each rank.
    let rime = abilities("rime");
    let Some(Ranked::PerRank(ranges)) = &rime.actions()["snow_owl"].range else {
        panic!("a range per rank");
    };
    assert_eq!(ranges[1], RangeField::Range(ActionRange::meters(half * 65)));
    let wraps = &husk.actions()["grasping_wraps"];
    assert_eq!(wraps.targeting, Targeting::Direction);
    let eleven = RangeField::Range(ActionRange::meters(Num::int(11)));
    assert_eq!(wraps.range, Some(Ranked::One(eleven)));
}

#[test]
fn lash_out_from_its_package_hits_exactly() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let lash_out = arena.action(&game, "hero-husk", "lash_out");
    let caster = caster(&arena, &mut game, lash_out, 3, Kit::Mana100, ());
    let near = spawn(&arena, &mut game, 3, Team::new(1), ());
    let far = spawn(&arena, &mut game, 4, Team::new(1), ());
    arena.tick(&mut game, 0, &[cast(caster, ActionTarget::None)]);
    // Rank 3 deals 125 within 3.5 m: 500 → 375 at 3 m, and nothing at 4 m.
    assert_eq!(health(&arena, &game, near), 375);
    assert_eq!(health(&arena, &game, far), 500);
}

#[test]
fn kenshos_twin_cut_hits_twice_on_each_seventh_attack_and_never_answers_itself() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let player = Owner::new(PlayerSlot::new(0));
    let blade = spawn_with(&arena, &mut game, 0, Team::new(0), Kit::Blade, player);
    let dummy = spawn(&arena, &mut game, 1, Team::new(1), ());
    let twin_cut = arena.modifier(&game, "hero-kensho", "twin_cut");
    Stats::give_modifier(&mut game, blade, twin_cut, Some((blade, None, 1)), true);
    // 20 an attack, every 12 ticks from tick 0, so 7 within 4 s, 120 ticks, are consecutive: 6
    // by tick 60, 500 − 120; the 7th, in tick 72, hits twice, − 40; its extra hit counts for
    // nothing, so the next 6 take 120 by tick 144, and the 14th, in tick 156, hits twice, − 40.
    arena.tick(&mut game, 0, &[attack(blade, dummy)]);
    let mut healths = vec![health(&arena, &game, dummy)];
    for _ in 1..=156 {
        arena.step(&mut game);
        healths.push(health(&arena, &game, dummy));
    }
    let at = |tick: usize| healths[tick];
    assert_eq!(
        [at(60), at(71), at(72), at(144), at(155), at(156)],
        [380, 380, 340, 220, 220, 180]
    );
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

/// Puts `action` at rank 1 in slot 0 of `id`, before the weapon its type gives it.
fn arm_with(game: &mut Match, id: StableId, action: ActionId) {
    let entity = game.entity_of(id).unwrap();
    let weapon = game.get::<ActionSlots>(entity).unwrap().slot(0).unwrap();
    let slots = ActionSlots::new([
        (action, SlotKind::new(0), Some(Rank::FIRST)),
        (weapon.action.unwrap(), weapon.kind, weapon.rank),
    ]);
    game.entity_mut(entity).insert(TimedSlots::new(slots));
}

/// Whether `id` has the toggle of its slot 0 on.
fn toggled(game: &Match, id: StableId) -> bool {
    let entity = game.entity_of(id).unwrap();
    let times = game.get::<SlotTimes>(entity).unwrap();
    times.slot(0).unwrap().toggle.is_some()
}

#[test]
fn rimes_chill_arrows_pays_mana_each_attack_and_turns_off_when_it_cannot() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let chill = arena.action(&game, "hero-rime", "chill_arrows");
    let hold = arena.modifier(&game, "hero-rime", "chill_arrows");
    let player = Owner::new(PlayerSlot::new(0));
    // Rime with 20 mana, a weapon of 10 every 12 ticks; an enemy beside her.
    let rime = spawn_with(&arena, &mut game, 0, Team::new(0), Kit::Archer, player);
    let enemy = spawn(&arena, &mut game, 1, Team::new(1), ());
    arm_with(&mut game, rime, chill);
    let mana = |game: &Match| pool(&arena, game, rime, "mana").round();
    let holds = |game: &Match| carried(game, rime).contains(&(hold, Some(rime)));
    // Tick 0: on, at no cost but its toggle's, and its hold from that tick's Resolve.
    arena.tick(&mut game, 0, &[cast(rime, ActionTarget::None)]);
    assert_eq!(
        (toggled(&game, rime), holds(&game), mana(&game)),
        (true, true, 20)
    );
    // Each attack pays 8 as it goes off, in ticks 1 and 13: 4 are left. The third, in tick 25,
    // cannot pay: the toggle turns off before it, and its hold with it.
    arena.tick(&mut game, 0, &[attack(rime, enemy)]);
    for _ in 2..=13 {
        arena.step(&mut game);
    }
    assert_eq!((toggled(&game, rime), mana(&game)), (true, 4));
    for _ in 14..=25 {
        arena.step(&mut game);
    }
    assert_eq!(
        (toggled(&game, rime), holds(&game), mana(&game)),
        (false, false, 4)
    );
    assert_eq!(health(&arena, &game, enemy), 470);
    // On again, then a second cast turns it off as it starts, at no cost.
    arena.tick(&mut game, 0, &[cast(rime, ActionTarget::None)]);
    assert!(toggled(&game, rime));
    arena.tick(&mut game, 0, &[cast(rime, ActionTarget::None)]);
    assert_eq!((toggled(&game, rime), mana(&game)), (false, 4));
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn rimes_chill_arrows_pays_for_no_attack_that_does_not_strike() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let chill = arena.action(&game, "hero-rime", "chill_arrows");
    let player = Owner::new(PlayerSlot::new(0));
    // Rime with 20 mana and a weapon of a 3-tick windup that costs 12; an enemy beside her.
    let rime = spawn_with(
        &arena,
        &mut game,
        0,
        Team::new(0),
        Kit::CostlyArcher,
        player,
    );
    let enemy = spawn(&arena, &mut game, 1, Team::new(1), ());
    arm_with(&mut game, rime, chill);
    let mana = |game: &Match| pool(&arena, game, rime, "mana").round();
    // On in tick 0; the attack starts in tick 1, its windup to end in tick 4.
    arena.tick(&mut game, 0, &[cast(rime, ActionTarget::None)]);
    arena.tick(&mut game, 0, &[attack(rime, enemy)]);
    // 10 mana go during the windup: as it ends, she cannot pay its 12, so it does not strike,
    // and her toggle, which pays as an attack strikes, takes nothing of the 10 left.
    let entity = game.entity_of(rime).unwrap();
    let mut pools = game.get_mut::<Pools>(entity).unwrap();
    pools.take(arena.pool("mana"), Num::int(10));
    for _ in 2..=4 {
        arena.step(&mut game);
    }
    assert_eq!((toggled(&game, rime), mana(&game)), (true, 10));
    assert_eq!(health(&arena, &game, enemy), 500);
}

#[test]
fn husks_dread_pays_mana_each_second_burns_enemies_near_and_ends_at_death() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let dread = arena.action(&game, "hero-husk", "dread");
    // Husk with 20 mana; an enemy of 500 health 2 m away, inside Dread's 3 m.
    let husk = caster(&arena, &mut game, dread, 1, Kit::Mana20, ());
    let enemy = spawn(&arena, &mut game, 2, Team::new(1), ());
    let mana = |game: &Match| pool(&arena, game, husk, "mana").round();
    // On in tick 0. It pays 8 at each whole second, in ticks 30 and 60: 4 are left. Its hold's
    // interval of 1000 ms deals 1.5% of the enemy's 500 health in ticks 30 and 60. In tick 90 it
    // cannot pay: it turns off before its hold's interval, which deals nothing more.
    arena.tick(&mut game, 0, &[cast(husk, ActionTarget::None)]);
    let mut seen = Vec::new();
    for tick in 1..=95 {
        arena.step(&mut game);
        if [30, 60, 90].contains(&tick) {
            seen.push((
                tick,
                toggled(&game, husk),
                mana(&game),
                pool(&arena, &game, enemy, "health"),
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
    arena.tick(&mut game, 0, &[cast(husk, ActionTarget::None)]);
    assert!(toggled(&game, husk));
    Combat::queue_damage(&mut game, None, husk, Num::int(1000), "true");
    arena.step(&mut game);
    arena.step(&mut game);
    assert!(!toggled(&game, husk));
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn kenshos_still_mind_heals_each_half_second_of_its_channel_and_guards_him_while_it_runs() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let still_mind = arena.action(&game, "hero-kensho", "still_mind");
    let guard = arena.modifier(&game, "hero-kensho", "still_mind");
    // Kensho with 100 mana, and 300 of his 500 health lost.
    let kensho = caster(&arena, &mut game, still_mind, 1, Kit::Mana100, ());
    let entity = game.entity_of(kensho).unwrap();
    let life = *game.get::<Life>(entity).unwrap();
    game.entity_mut(entity).insert(life.spent(Num::int(300)));
    let guarded = |game: &Match| carried(game, kensho).contains(&(guard, Some(kensho)));
    // Cast in tick 0, for 50 mana. The channel runs from tick 1 for 5000 ms, 150 ticks, and
    // ticks each 500 ms, 15 ticks: in ticks 16 to 151, ten times, each 20 at rank 1 with no
    // ability power. It guards him from tick 0's Resolve until it ends in tick 151.
    arena.tick(&mut game, 0, &[cast(kensho, ActionTarget::None)]);
    assert_eq!(
        (pool(&arena, &game, kensho, "mana"), guarded(&game)),
        (Num::int(50), true)
    );
    let mut healed = Vec::new();
    for tick in 1..=152 {
        arena.step(&mut game);
        if [15, 16, 150, 151].contains(&tick) {
            healed.push((tick, pool(&arena, &game, kensho, "health"), guarded(&game)));
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
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn veils_dusk_mark_detonates_once_on_veils_next_damage() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let action = arena.action(&game, "hero-veil", "dusk_mark");
    let dusk_mark = arena.modifier(&game, "hero-veil", "dusk_mark");
    let player = Owner::new(PlayerSlot::new(0));
    let veil = spawn_with(
        &arena,
        &mut game,
        0,
        Team::new(0),
        Kit::EnergyStriker,
        player,
    );
    let energy = arena.pool("energy");
    let entity = game.entity_of(veil).unwrap();
    let pools = *game.get::<Pools>(entity).unwrap();
    game.entity_mut(entity)
        .insert(pools.spent(energy, Num::int(100)));
    let other = spawn_with(&arena, &mut game, 0, Team::new(0), Kit::Striker, player);
    let marked = spawn(&arena, &mut game, 1, Team::new(1), ());
    let marks = |game: &Match| !carried(game, marked).is_empty();
    let from = Some((veil, Some(action), 2));
    Stats::give_modifier(&mut game, marked, dusk_mark, from, false);
    // Another unit's attack of 30 in tick 0 leaves the mark.
    arena.tick(&mut game, 0, &[attack(other, marked)]);
    assert_eq!((health(&arena, &game, marked), marks(&game)), (470, true));
    // Veil's attack of 30 in tick 1 detonates it at rank 2: 70 more, and 25 energy back, from 100
    // to 125. The detonation is Dusk Mark's own damage, and the mark is gone: it detonates once.
    arena.tick(&mut game, 0, &[attack(veil, marked)]);
    assert_eq!((health(&arena, &game, marked), marks(&game)), (370, false));
    assert_eq!(pool(&arena, &game, veil, "energy"), Num::int(125));
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn rimes_fan_of_frost_from_its_package_hits_exactly_the_units_in_reach_once_each() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    // Fan of Frost runs no script: its data's `on_hit` deals its damage and applies its slow.
    assert_eq!(
        abilities(arena.packages(), "rime").actions()["fan_of_frost"].script,
        None
    );
    let fan = arena.action(&game, "hero-rime", "fan_of_frost");
    let caster = caster(&arena, &mut game, fan, 1, Kit::Mana100, ());
    // Seven arrows from the origin, 57.5° apart from the first to the last, around +x: at 0°,
    // ±9.583°, ±19.167° and ±28.75°, each 0.4 m wide, so a body of no radius within 0.2 m of an
    // arrow's path is in reach, up to the 12 m of the range.
    let enemy = |game: &mut Match, at| spawn_at(&arena, game, at, Team::new(1), Kit::Bare, ());
    // 1 m out on +x, 1 · sin 9.583° = 0.166 m from the arrows beside the middle one: three
    // arrows reach it in the same tick, and the cast hits it once.
    let near = enemy(&mut game, milli(1000, 0));
    // On +x at 10 m: the middle arrow, which the near one did not stop, as another arrow of its
    // cast hit that one first.
    let far = enemy(&mut game, milli(10_000, 0));
    // 7.998 m out at ±28.774°, 0.003 m from the outer arrows.
    let outer = [3850, -3850].map(|z| enemy(&mut game, milli(7010, z)));
    // 12.5 m out at ±19.167°, past the range: 0.5 m from where the arrows end.
    let beyond = [4104, -4104].map(|z| enemy(&mut game, milli(11_807, z)));
    // 6 m out at ±4.78°, between two arrows: 0.50 m from each.
    let between = [500, -500].map(|z| enemy(&mut game, milli(5980, z)));
    let ally = spawn_at(
        &arena,
        &mut game,
        milli(5000, 0),
        Team::new(0),
        Kit::Bare,
        (),
    );

    arena.tick(
        &mut game,
        0,
        &[cast(caster, ActionTarget::Point(milli(10_000, 0)))],
    );
    // The cast resolves after its windup of 8 ticks, and the arrows fly 0.5 m a tick for 24.
    for _ in 0..40 {
        arena.step(&mut game);
    }
    assert_eq!(projectiles(&mut game), 0);

    // Rank 1 deals 40, and slows.
    let slowed = |id: StableId| !carried(&game, id).is_empty();
    let hit = [near, far, outer[0], outer[1]];
    let missed = [beyond[0], beyond[1], between[0], between[1], ally];
    assert_eq!(hit.map(|unit| health(&arena, &game, unit)), [460; 4]);
    assert_eq!(missed.map(|unit| health(&arena, &game, unit)), [500; 5]);
    assert_eq!(hit.map(slowed), [true; 4]);
    assert_eq!(missed.map(slowed), [false; 5]);
    assert_eq!(pool(&arena, &game, caster, "mana"), Num::int(40));
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn rimes_snow_owl_flies_to_its_point_and_ends_there_and_its_sight_lingers() {
    let arena = arena();
    let mut game = arena.start_seeing(RATE, 1, 2);
    let snow_owl = arena.action(&game, "hero-rime", "snow_owl");
    let caster = caster(&arena, &mut game, snow_owl, 1, Kit::Bare, ());
    // An enemy at (10, 0): its cell's center (10.5, 0.5) is √12.5 ≈ 3.54 m from the point, within
    // the owl's sight of 4 m and its linger's radius of 4 m; the caster has no sight of its own.
    let enemy = spawn(&arena, &mut game, 10, Team::new(1), ());
    let seen = |game: &Match| {
        let entity = game.entity_of(enemy).unwrap();
        game.get::<SeenBy>(entity)
            .unwrap()
            .get()
            .unwrap()
            .contains(Team::new(0))
    };
    let owls = |game: &mut Match| -> Vec<Vec3> {
        let mut owls = game.query::<(&Projectile, &Position)>();
        owls.iter(game).map(|(_, at)| at.get()).collect()
    };

    // Aimed at a point 7 m out, within its 25 m range: it launches in tick 0, and flies 14/30 m
    // a tick, 7 829 367 bits rounded, from tick 1. Fifteen steps are 7 bits short of 7 m, and the
    // sixteenth ends it on the point, where its `on_end` runs.
    let point = Vec3::from((7, 0, 0));
    arena.tick(&mut game, 0, &[cast(caster, ActionTarget::Point(point))]);
    assert!(!seen(&game));
    for _ in 1..=15 {
        arena.step(&mut game);
    }
    let short = Num::from_bits(15 * 7_829_367);
    assert_eq!(short, Num::int(7) - Num::from_bits(7));
    let flying = Vec3::new(short, Num::ZERO, Num::ZERO);
    assert_eq!(owls(&mut game), [flying]);
    arena.step(&mut game);
    assert_eq!(owls(&mut game), []);
    // Its `on_end` in tick 16 reveals 4 m round the point for 5000 ms, 150 ticks: the Vision
    // stages of ticks 16 to 165, with the owl gone.
    assert!(seen(&game));
    for _ in 17..=165 {
        arena.step(&mut game);
    }
    assert!(seen(&game));
    arena.step(&mut game);
    assert!(!seen(&game));
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn farsight_shows_its_cells_to_the_caster_team_alone_for_its_time() {
    let arena = arena();
    let mut game = arena.start_seeing(RATE, 1, 3);
    let farsight = arena.action(&game, "player-spells", "farsight");
    let caster = caster(&arena, &mut game, farsight, 1, Kit::Bare, ());
    // Revealed 6 m round (20, 0): the cell of (25, 0), its center (25.5, 0.5) √30.5 ≈ 5.52 m
    // off, and not that of (26, 0), at (26.5, 0.5) √42.5 ≈ 6.52 m off. Team 2 sees neither.
    let near = spawn(&arena, &mut game, 25, Team::new(1), ());
    let far = spawn(&arena, &mut game, 26, Team::new(1), ());
    let seen = |game: &Match, id: StableId| {
        let entity = game.entity_of(id).unwrap();
        let teams = game.get::<SeenBy>(entity).unwrap().get().unwrap();
        [0, 2].map(|team| teams.contains(Team::new(team)))
    };
    // It resolves at once, in tick 0, and reveals for 5000 ms, 150 ticks: ticks 0 to 149.
    let point = Vec3::from((20, 0, 0));
    arena.tick(&mut game, 0, &[cast(caster, ActionTarget::Point(point))]);
    assert_eq!(
        [near, far].map(|unit| seen(&game, unit)),
        [[true, false], [false; 2]]
    );
    for _ in 1..=149 {
        arena.step(&mut game);
    }
    assert_eq!(seen(&game, near), [true, false]);
    arena.step(&mut game);
    assert_eq!(seen(&game, near), [false; 2]);
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
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
    let arena = arena();
    let mut game = arena.start(rate, 1);
    let eruption = arena.action(&game, "hero-cinder", "eruption");
    let kindle = arena.modifier(&game, "hero-cinder", "kindle");
    let caster = caster(&arena, &mut game, eruption, 1, Kit::Mana100, ());
    // The area lands on (6, 0, 0), of radius 2.4, and these bodies have no radius: one on the
    // center and one 2.4 m out are in reach, one 2.401 m out is not, and an ally is never hit.
    let enemy = |game: &mut Match, at| spawn_at(&arena, game, at, Team::new(1), Kit::Bare, ());
    let center = enemy(&mut game, milli(6000, 0));
    let edge = enemy(&mut game, milli(8400, 0));
    let beyond = enemy(&mut game, milli(6000, 2401));
    let burning = enemy(&mut game, milli(6000, 1000));
    let ally = spawn_at(
        &arena,
        &mut game,
        milli(6000, 0),
        Team::new(0),
        Kit::Bare,
        (),
    );
    let from = Some((caster, Some(eruption), 1));
    Stats::give_modifier(&mut game, burning, kindle, from, false);

    arena.tick(
        &mut game,
        0,
        &[cast(caster, ActionTarget::Point(milli(6000, 0)))],
    );
    let units = [center, edge, beyond, burning, ally];
    for _ in 1..erupts {
        arena.step(&mut game);
    }
    assert_eq!(
        units.map(|unit| health(&arena, &game, unit)),
        [500; 5],
        "{rate:?}"
    );
    assert_eq!(areas(&mut game), 1);
    arena.step(&mut game);
    // Rank 1 deals 75, and 1.25 × 75 = 93.75 to a unit ablaze: 500 → 406.25. Each unit hit is
    // ablaze after, and the area ends with its eruption.
    assert_eq!(
        units.map(|unit| health(&arena, &game, unit)),
        [425, 425, 500, 406, 500],
        "{rate:?}"
    );
    let ablaze = |unit| carried(&game, unit) == [(kindle, Some(caster))];
    assert_eq!(units.map(ablaze), [true, true, false, true, false]);
    assert_eq!(areas(&mut game), 0);
    assert_eq!(pool(&arena, &game, caster, "mana"), Num::int(30));
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn cinders_chain_fire_bounces_between_two_enemies_until_its_bounces_run_out() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let chain_fire = arena.action(&game, "hero-cinder", "chain_fire");
    let caster = caster(&arena, &mut game, chain_fire, 1, Kit::Mana100, ());
    // Two enemies 4 m apart, within the 5 m of a bounce. The arena has no map, so its Vision
    // stage never runs: each is seen by every team, as a bounce picks among what its caster's
    // team sees.
    let near = spawn(&arena, &mut game, 4, Team::new(1), SeenBy::all());
    let far = spawn(&arena, &mut game, 8, Team::new(1), SeenBy::all());
    let projectiles = |game: &mut Match| -> Vec<u64> {
        let mut query = game.query::<(&Projectile, &StableId)>();
        query.iter(game).map(|(_, id)| id.get()).collect()
    };

    // The cast's projectile, id 3 after the caster's 0 and the enemies' 1 and 2, flies at 10 m a
    // second, a third of a meter a tick rounded down, so 4 m takes 13 steps: it hits `near` in
    // tick 13. Each hit deals 150 at rank 1 and, with its type's 4 bounces left and counting
    // down, picks the one other enemy in reach: 4 bounces after the first hit, each 4 m and 13
    // ticks, alternating, ids 4 to 7. The last hit, with none left, bounces no more. Each hit
    // also kindles its target, which burns 2 % of its 500 health, 10, each 30 ticks from the
    // first: `near`, hit in tick 13, in ticks 43 and 73, and `far`, hit in tick 26, in tick 56.
    arena.tick(&mut game, 0, &[cast(caster, ActionTarget::Unit(near))]);
    let mut flown = Vec::new();
    let mut hits = Vec::new();
    let mut last = [500, 500];
    for tick in 1..=80 {
        flown.extend(projectiles(&mut game));
        arena.step(&mut game);
        let now = [near, far].map(|unit| health(&arena, &game, unit));
        if now != last {
            hits.push((tick, now));
            last = now;
        }
    }
    flown.dedup();
    assert_eq!(flown, [3, 4, 5, 6, 7]);
    assert_eq!(projectiles(&mut game), Vec::<u64>::new());
    assert_eq!(
        hits,
        [
            (13, [350, 500]),
            (26, [350, 350]),
            (39, [200, 350]),
            (43, [190, 350]),
            (52, [190, 200]),
            (56, [190, 190]),
            (65, [40, 190]),
            (73, [30, 190]),
        ]
    );
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn veils_smoke_ring_from_its_package_holds_its_modifiers_on_the_units_inside_while_it_lasts() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    assert_eq!(
        abilities(arena.packages(), "veil").actions()["smoke_ring"].script,
        None
    );
    let ring = arena.action(&game, "hero-veil", "smoke_ring");
    let cover = arena.modifier(&game, "hero-veil", "smoke_ring_cover");
    let slow = arena.modifier(&game, "hero-veil", "smoke_ring_slow");
    let caster = caster(&arena, &mut game, ring, 1, Kit::Energy100, ());
    let unit =
        |game: &mut Match, team, x| spawn_with(&arena, game, x, Team::new(team), Kit::Bare, ());
    // The ring lands where Veil stands, of radius 3: an enemy 2 m and one 3 m away are inside,
    // one 4 m away is not, and an ally inside gets nothing, as the ring names no `allies`.
    let near = unit(&mut game, 1, 2);
    let edge = unit(&mut game, 1, 3);
    let far = unit(&mut game, 1, 4);
    let ally = unit(&mut game, 0, 1);
    let held = |game: &Match| [caster, near, edge, far, ally].map(|unit| carried(game, unit));
    let from_veil = |id| vec![(id, Some(caster))];
    let mut expected = [
        from_veil(cover),
        from_veil(slow),
        from_veil(slow),
        vec![],
        vec![],
    ];

    // The cast resolves at once and the ring lands in tick 0; it holds its modifiers from that
    // tick's Resolve. It lasts 8000 ms, 240 ticks, and ends in tick 240.
    arena.tick(&mut game, 0, &[cast(caster, ActionTarget::None)]);
    assert_eq!(held(&game), expected);
    assert_eq!(pool(&arena, &game, caster, "energy"), Num::int(20));
    // The near enemy steps out to 4 m: its slow ends in the next tick's Resolve.
    for _ in 1..100 {
        arena.step(&mut game);
    }
    let entity = game.entity_of(near).unwrap();
    *game.get_mut::<Position>(entity).unwrap() = Position::from((4, 0, 0));
    arena.step(&mut game);
    expected[1] = vec![];
    assert_eq!(held(&game), expected);
    for _ in 101..240 {
        arena.step(&mut game);
    }
    assert_eq!(held(&game), expected);
    assert_eq!(areas(&mut game), 1);
    arena.step(&mut game);
    assert_eq!(held(&game), [vec![], vec![], vec![], vec![], vec![]]);
    assert_eq!(areas(&mut game), 0);
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

/// A body of half a meter.
fn body() -> Body {
    Body::new(Num::ONE / 2).unwrap()
}

/// A body of half a meter that walks a meter a tick.
fn walker() -> impl Bundle {
    let step = MoveStep::new(Num::ONE).unwrap();
    (body(), Navigation::walker(step))
}

/// Where `id` stands along x, and the forced move under way on it.
fn forced(game: &Match, id: StableId) -> (Num, Option<ForcedMove>) {
    let entity = game.entity_of(id).unwrap();
    let x = game.get::<Position>(entity).unwrap().x;
    (x, game.get::<ForcedMove>(entity).copied())
}

#[test]
fn veils_night_step_dashes_to_its_target_and_strikes_as_the_dash_ends_beside_it() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let night_step = arena.action(&game, "hero-veil", "night_step");
    let veil = caster(&arena, &mut game, night_step, 1, Kit::Energy200, walker());
    let enemy = spawn(&arena, &mut game, 6, Team::new(1), body());

    // It resolves at once in tick 0, and its dash of 22 m a second goes 22/30 m a tick, s, from
    // tick 1, until the bodies of half a meter touch, 1 m apart. Six steps leave 6 − 6s = 1.6 m,
    // within 1 + s; the seventh, in tick 7, ends the dash on 5 m, and its `on_end` deals rank 1's
    // 100 magic, with no ability power, in that tick.
    arena.tick(&mut game, 0, &[cast(veil, ActionTarget::Unit(enemy))]);
    for _ in 1..=6 {
        arena.step(&mut game);
    }
    let (x, dash) = forced(&game, veil);
    let step = Num::int(22) / 30;
    assert_eq!((x, health(&arena, &game, enemy)), (step * 6, 500));
    assert!(matches!(
        dash,
        Some(ForcedMove::Dash {
            delivers: Some(_),
            ..
        })
    ));
    arena.step(&mut game);
    assert_eq!(forced(&game, veil), (Num::int(5), None));
    assert_eq!(health(&arena, &game, enemy), 400);
    for _ in 0..30 {
        arena.step(&mut game);
    }
    assert_eq!(health(&arena, &game, enemy), 400);
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}

#[test]
fn husks_grasping_wraps_pulls_him_to_the_enemy_hit_and_its_pull_delivers_nothing() {
    let arena = arena();
    let mut game = arena.start(RATE, 1);
    let wraps = arena.action(&game, "hero-husk", "grasping_wraps");
    let husk = caster(&arena, &mut game, wraps, 1, Kit::Mana200, walker());
    let enemy = spawn(&arena, &mut game, 5, Team::new(1), body());

    // The cast resolves after its windup of 250 ms, 8 ticks, and its wraps fly 20/30 m a tick
    // from tick 9, 1.6 m wide: they reach the enemy's body once their front comes within 0.8 +
    // 0.5 m of its center, in their sixth step, tick 14. Its `on_hit` deals rank 1's 80, stuns,
    // and pulls Husk at 18 m a second: a dash from the delivery's hit, not from `on_resolve`, so
    // it delivers no action and its end runs no `on_end`.
    arena.tick(
        &mut game,
        0,
        &[cast(husk, ActionTarget::Point(milli(5000, 0)))],
    );
    let mut pulled = None;
    for tick in 1..=40 {
        arena.step(&mut game);
        if pulled.is_none() && forced(&game, husk).1.is_some() {
            pulled = Some((tick, forced(&game, husk).1.unwrap()));
        }
    }
    let (tick, pull) = pulled.unwrap();
    assert_eq!(tick, 14);
    assert!(matches!(pull, ForcedMove::Dash { delivers: None, .. }));
    // The pull ends with the bodies touching, Husk on 4 m, and the enemy took the hit alone.
    assert_eq!(forced(&game, husk), (Num::int(4), None));
    assert_eq!(health(&arena, &game, enemy), 420);
    assert!(
        arena.failures(&game).is_empty(),
        "{:?}",
        arena.failures(&game)
    );
}
