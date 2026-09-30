//! The reference heroes' abilities as their packages hold them: every ability's data reads into
//! the typed schema, and Husk's Lash Out, loaded from its data file and script, hits exactly.

use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::World;
use campfire_capabilities::{
    Abilities, AbilitySlots, AbilityTables, Action, AttackStats, CastTarget, Combat, Combatant,
    Control, Controller, Health, Navigation, OnDeath, Order, Param, Range, Ranked, ResourcePool,
    Scalar, Scaling, Targeting, Team,
};
use campfire_content::{PackageDir, PackagePath};
use campfire_math::{Num, SegmentSeed, Vec3};
use campfire_script::ScriptLimits;
use campfire_sim::{
    EntityIndex, IdAllocator, Position, SimUpdate, StableId, StateRegistry, TickInput, TickInputs,
};

const HEROES: [&str; 6] = ["cinder", "gale", "husk", "kensho", "rime", "veil"];

fn hero(name: &str) -> PackageDir {
    PackageDir::new(format!(
        "{}/../../packages/moba/heroes/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// The abilities of the hero `name`, as its data file declares them.
fn abilities(name: &str) -> AbilityTables {
    let path = PackagePath::parse("data/hero.toml").unwrap();
    hero(name).read_data(&path).unwrap()
}

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// A unit of 500 health on `team` at `x` meters along x, with `parts`.
fn spawn(world: &mut World, team: u8, x: i64, parts: impl Bundle) -> StableId {
    let id = world.resource_mut::<IdAllocator>().allocate();
    let combatant = Combatant {
        health: Health::new(num(500)).unwrap(),
        attack: AttackStats::new(Num::ZERO, 0, 1, Num::ZERO).unwrap(),
        on_death: OnDeath::Stay,
    };
    let at = Position::new(Vec3::new(num(x), Num::ZERO, Num::ZERO)).unwrap();
    world.spawn((id, at, combatant.bundle(Team::new(team)), parts));
    id
}

#[test]
fn every_reference_ability_reads_into_the_schema() {
    let mut read = 0;
    for name in HEROES {
        let data = abilities(name);
        for (ability, data) in &data.abilities {
            if let Some(script) = &data.script {
                assert!(hero(name).read_text(script).is_ok(), "{name}.{ability}");
            }
            read += 1;
        }
    }
    // Four abilities a hero.
    assert_eq!(read, 24);

    // Husk's Lash Out reads exactly as its file writes it.
    let husk = abilities("husk");
    let lash_out = &husk.abilities["lash_out"];
    assert_eq!(lash_out.targeting, Targeting::None);
    assert_eq!(lash_out.range, None);
    assert_eq!(
        lash_out.cooldown_ms,
        Some(Ranked::PerRank(vec![10_000, 9000, 8000, 7000, 6000]))
    );
    assert_eq!(lash_out.cost, Some(Ranked::One(35)));
    // "3.5" is 7 halves; "0.5" one half.
    let half = Num::from_bits(1 << 23);
    assert_eq!(
        lash_out.params["radius"],
        Param::Value(Scalar::Decimal(half * 7))
    );
    let Param::Scaling(Scaling { base, ap, .. }) = &lash_out.params["damage"] else {
        panic!("damage scales");
    };
    assert_eq!(base.at(2), Some(Scalar::Int(100)));
    assert_eq!(*ap, Some(Scalar::Decimal(half)));
    // Rime's Snow Owl reaches farther at each rank.
    let rime = abilities("rime");
    let Some(Ranked::PerRank(ranges)) = &rime.abilities["snow_owl"].range else {
        panic!("a range per rank");
    };
    assert_eq!(ranges[1], Range::Meters(half * 65));
    let wraps = &husk.abilities["grasping_wraps"];
    assert_eq!(wraps.targeting, Targeting::Direction);
    assert_eq!(wraps.range, Some(Ranked::One(Range::Meters(num(11)))));
}

#[test]
fn lash_out_from_its_package_hits_exactly() {
    let mut world = World::new();
    SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
    let mut schedule = SimUpdate::schedule();
    let mut registry = StateRegistry::new();
    Combat::install(&mut world, &mut schedule, &mut registry);
    Navigation::install(&mut world, &mut schedule, &mut registry);
    let limits = ScriptLimits {
        per_call: 10_000,
        per_tick: 100_000,
    };
    Abilities::install(&mut world, &mut schedule, &mut registry, limits);
    Control::install(&mut schedule, &mut registry);
    world.add_schedule(schedule);

    let husk = abilities("husk");
    let data = &husk.abilities["lash_out"];
    let source = hero("husk")
        .read_text(data.script.as_ref().unwrap())
        .unwrap();
    let lash_out = Abilities::load(&mut world, data, Some(&source), 30).unwrap();

    let caster = spawn(
        &mut world,
        0,
        0,
        (
            Controller::new(0),
            AbilitySlots::new([(lash_out, 3)]),
            ResourcePool::new(num(100)).unwrap(),
        ),
    );
    let near = spawn(&mut world, 1, 3, ());
    let far = spawn(&mut world, 1, 4, ());

    let payload = Order::payload(&[Order {
        unit: caster,
        action: Action::Cast {
            slot: 0,
            target: CastTarget::None,
        },
    }]);
    world.resource_mut::<TickInputs>().push(TickInput {
        slot: 0,
        payload: &payload,
    });
    world.run_schedule(SimUpdate);

    // Rank 3 deals 125 within 3.5 m: 500 → 375 at 3 m, and nothing at 4 m.
    let health = |world: &World, id: StableId| {
        let entity = world.resource::<EntityIndex>().get(id).unwrap();
        world.get::<Health>(entity).unwrap().current().round()
    };
    assert_eq!(health(&world, near), 375);
    assert_eq!(health(&world, far), 500);
}
