use bevy_ecs::entity::Entity;
use std::hint::black_box;

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use criterion::Criterion;
use serde::{Deserialize, Serialize};

use crate::entity_index::EntityIndex;
use crate::id_allocator::IdAllocator;
use crate::sim_state::SimComponent;
use crate::stable_id::StableId;
use crate::state_registry::StateRegistry;

/// Heroes, creeps, structures and projectiles of a 3v3 match at its busiest.
const ENTITIES: i64 = 300;

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
    let mut registry = StateRegistry::new();
    registry.register_component::<Position>();
    registry.register_component::<Velocity>();
    registry.register_component::<Health>();
    registry.register_component::<Mana>();
    registry.register_component::<Team>();
    registry.register_component::<Target>();
    registry.register_component::<Cooldowns>();
    registry.register_component::<Stats>();
    let mut per_type = Vec::new();

    let mut group = c.benchmark_group("state_hash");
    group.bench_function("moba_300", |b| {
        b.iter(|| black_box(registry.hash(black_box(&world))));
    });
    group.bench_function("moba_300_by_type", |b| {
        b.iter(|| black_box(registry.hash_by_type(black_box(&world), &mut per_type)));
    });
    group.finish();
}
