use std::hint::black_box;

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use criterion::Criterion;
use serde::Serialize;

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::sim_state::SimComponent;
use crate::stable_id::StableId;
use crate::state_hasher::StateHasher;

/// Heroes, creeps, structures and projectiles of a 3v3 match at its busiest.
const ENTITIES: i64 = 300;

#[derive(Component, Debug, Serialize)]
struct Position(Vec3);

#[derive(Component, Debug, Serialize)]
struct Velocity(Vec3);

#[derive(Component, Debug, Serialize)]
struct Health(Num, Num);

#[derive(Component, Debug, Serialize)]
struct Mana(Num, Num);

#[derive(Component, Debug, Serialize)]
struct Team(u8);

#[derive(Component, Debug, Serialize)]
struct Target(Option<StableId>);

#[derive(Component, Debug, Serialize)]
struct Cooldowns([u32; 6]);

#[derive(Component, Debug, Serialize)]
struct Stats([Num; 12]);

impl SimComponent for Position {
    const NAME: &'static str = "bench.position";
}

impl SimComponent for Velocity {
    const NAME: &'static str = "bench.velocity";
}

impl SimComponent for Health {
    const NAME: &'static str = "bench.health";
}

impl SimComponent for Mana {
    const NAME: &'static str = "bench.mana";
}

impl SimComponent for Team {
    const NAME: &'static str = "bench.team";
}

impl SimComponent for Target {
    const NAME: &'static str = "bench.target";
}

impl SimComponent for Cooldowns {
    const NAME: &'static str = "bench.cooldowns";
}

impl SimComponent for Stats {
    const NAME: &'static str = "bench.stats";
}

fn moba_world() -> World {
    let mut world = World::new();
    world.init_resource::<EntityIndex>();
    world.init_resource::<IdAllocator>();
    for i in 0..ENTITIES {
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

pub fn state_hash(c: &mut Criterion) {
    let world = moba_world();
    let mut hasher = StateHasher::new();
    hasher.register_component::<Position>();
    hasher.register_component::<Velocity>();
    hasher.register_component::<Health>();
    hasher.register_component::<Mana>();
    hasher.register_component::<Team>();
    hasher.register_component::<Target>();
    hasher.register_component::<Cooldowns>();
    hasher.register_component::<Stats>();
    let mut per_type = Vec::new();

    let mut group = c.benchmark_group("state_hash");
    group.bench_function("moba_300", |b| {
        b.iter(|| black_box(hasher.hash(black_box(&world))));
    });
    group.bench_function("moba_300_by_type", |b| {
        b.iter(|| black_box(hasher.hash_by_type(black_box(&world), &mut per_type)));
    });
    group.finish();
}
