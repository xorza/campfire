use bevy_ecs::entity::Entity;
use std::hint::black_box;

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use criterion::{Criterion, Throughput};
use serde::{Deserialize, Serialize};

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::sim_state::SimComponent;
use crate::stable_id::StableId;
use crate::state_registry::StateRegistry;

/// The units of a kernel case, as an RTS battle holds them.
const UNITS: i64 = 1000;

#[derive(Component, Debug, Serialize, Deserialize)]
struct Position(Vec3);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Velocity(Vec3);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Health(Num, Num);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Mana(Num, Num);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Team(u8);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Target(Option<StableId>);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Cooldowns([u32; 6]);

#[derive(Component, Debug, Serialize, Deserialize)]
struct Stats([Num; 12]);

impl SimComponent for Position {
    const NAME: &'static str = "bench.position";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Velocity {
    const NAME: &'static str = "bench.velocity";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Health {
    const NAME: &'static str = "bench.health";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Mana {
    const NAME: &'static str = "bench.mana";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "bench.team";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Target {
    const NAME: &'static str = "bench.target";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Cooldowns {
    const NAME: &'static str = "bench.cooldowns";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Stats {
    const NAME: &'static str = "bench.stats";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

/// `UNITS` units, each with a position, a velocity, life, a team and a target, and one in 50
/// with mana, cooldowns and a row of stats, as a hero or a tower has.
fn units_world() -> World {
    let mut world = World::new();
    world.init_resource::<EntityIndex>();
    world.init_resource::<IdAllocator>();
    for i in 0..UNITS {
        let at = Num::from_int(i).unwrap();
        let id = world.resource_mut::<IdAllocator>().allocate();
        let mut entity = world.spawn((
            id,
            Position(Vec3::new(at, Num::ZERO, at)),
            Velocity(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO)),
            Health(at, at),
            Team(u8::from(i % 2 == 0)),
            Target(None),
        ));
        if i % 50 == 0 {
            entity.insert((
                Mana(at, at),
                Cooldowns([i.unsigned_abs().try_into().unwrap(); 6]),
                Stats([at; 12]),
            ));
        }
    }
    world
}

/// The state hash of a world of `UNITS` units: over all of it, as a checkpoint takes it, and split
/// by state type, as the copy checks take it to name the type that differs.
pub(crate) fn state_hash(c: &mut Criterion) {
    let world = units_world();
    let registry = units_registry();
    let mut per_type = Vec::new();

    let mut group = c.benchmark_group("integration/state_hash");
    group.throughput(Throughput::Elements(UNITS.unsigned_abs()));
    group.bench_function("all", |b| {
        b.iter(|| black_box(registry.hash(black_box(&world))));
    });
    group.bench_function("by_type", |b| {
        b.iter(|| black_box(registry.hash_by_type(black_box(&world), &mut per_type)));
    });
    group.finish();
}

/// The snapshot of a world of `UNITS` units, as a checkpoint writes it.
pub(crate) fn snapshot(c: &mut Criterion) {
    let world = units_world();
    let registry = units_registry();
    let mut out = Vec::new();

    let mut group = c.benchmark_group("integration/snapshot");
    group.throughput(Throughput::Elements(UNITS.unsigned_abs()));
    group.bench_function("all", |b| {
        b.iter(|| {
            registry.snapshot(black_box(&world), &mut out);
            black_box(&out);
        });
    });
    group.finish();
}

/// The registry of `units_world`'s state types.
fn units_registry() -> StateRegistry {
    let mut registry = StateRegistry::new();
    registry.register_component::<Position>();
    registry.register_component::<Velocity>();
    registry.register_component::<Health>();
    registry.register_component::<Mana>();
    registry.register_component::<Team>();
    registry.register_component::<Target>();
    registry.register_component::<Cooldowns>();
    registry.register_component::<Stats>();
    registry
}
