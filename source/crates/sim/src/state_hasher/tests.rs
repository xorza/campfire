use bevy_ecs::component::Component;
use campfire_math::{Num, Vec3};
use serde::Serialize;

use super::*;
use crate::stable_id::StableId;

#[derive(Component, Debug, Clone, Copy, Serialize)]
struct Health(Num);

impl SimComponent for Health {
    const NAME: &'static str = "test.health";
}

#[derive(Component, Debug, Clone, Copy, Serialize)]
struct Position(Vec3);

impl SimComponent for Position {
    const NAME: &'static str = "test.position";
}

/// Stands for a component the network layer adds, which is not state.
#[derive(Component, Debug)]
struct Replicated;

fn new_world() -> World {
    let mut world = World::new();
    world.init_resource::<EntityIndex>();
    world.init_resource::<IdAllocator>();
    world
}

fn hasher() -> StateHasher {
    let mut hasher = StateHasher::new();
    hasher.register_component::<Position>();
    hasher.register_component::<Health>();
    hasher
}

fn allocate(world: &mut World) -> StableId {
    world.resource_mut::<IdAllocator>().allocate()
}

fn health(value: i64) -> Health {
    Health(Num::from_int(value).unwrap())
}

fn position(x: i64) -> Position {
    Position(Vec3::new(Num::from_int(x).unwrap(), Num::ZERO, Num::ZERO))
}

fn by_type(world: &World) -> Vec<TypeHash> {
    let mut per_type = Vec::new();
    hasher().hash_by_type(world, &mut per_type);
    per_type
}

/// Two entities, spawned in id order with all components at once.
fn plain_world() -> World {
    let mut world = new_world();
    let first = allocate(&mut world);
    let second = allocate(&mut world);
    world.spawn((first, health(10), position(1)));
    world.spawn((second, health(20)));
    world
}

/// The same state built another way: a non-sim entity first, the second id spawned first, and
/// components inserted one by one in another order.
fn shuffled_world() -> World {
    let mut world = new_world();
    world.spawn(Replicated);
    let first = allocate(&mut world);
    let second = allocate(&mut world);
    world.spawn(second).insert(health(20));
    world.spawn(first).insert(position(1)).insert(health(10));
    world
}

#[test]
fn types_hash_in_name_order() {
    let names: Vec<_> = by_type(&plain_world()).iter().map(|t| t.name).collect();
    assert_eq!(names, ["sim.id_allocator", "test.health", "test.position"]);
}

#[test]
fn build_order_does_not_matter() {
    let hasher = hasher();
    assert_eq!(hasher.hash(&plain_world()), hasher.hash(&shuffled_world()));
}

#[test]
fn one_changed_field_changes_only_its_type() {
    let mut world = shuffled_world();
    let before = by_type(&world);
    let entity = world.resource::<EntityIndex>().iter().next().unwrap().1;
    world.get_mut::<Health>(entity).unwrap().0 += Num::EPSILON;
    let after = by_type(&world);
    let changed: Vec<_> = before
        .iter()
        .zip(&after)
        .filter(|(b, a)| b.hash != a.hash)
        .map(|(b, _)| b.name)
        .collect();
    assert_eq!(changed, ["test.health"]);
    assert_ne!(hasher().hash(&world), hasher().hash(&shuffled_world()));
}

#[test]
fn non_sim_components_change_nothing() {
    let mut world = plain_world();
    let before = hasher().hash(&world);
    let entities: Vec<_> = world
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, e)| e)
        .collect();
    for entity in entities {
        world.entity_mut(entity).insert(Replicated);
    }
    world.spawn(Replicated);
    assert_eq!(hasher().hash(&world), before);
}

#[test]
fn allocator_state_is_hashed() {
    let mut world = plain_world();
    let before = by_type(&world);
    allocate(&mut world);
    let after = by_type(&world);
    let changed: Vec<_> = before
        .iter()
        .zip(&after)
        .filter(|(b, a)| b.hash != a.hash)
        .map(|(b, _)| b.name)
        .collect();
    assert_eq!(changed, ["sim.id_allocator"]);
}

fn assert_writer_matches<T: Serialize>(value: &T) {
    let mut expected = Hasher::new();
    expected.update(&postcard::to_allocvec(value).unwrap());
    let mut actual = Hasher::new();
    write(&mut actual, value);
    assert_eq!(actual.finalize(), expected.finalize());
}

#[test]
fn writer_matches_postcard_bytes() {
    // 12 `Num`s of 10 bytes each exceed the 64-byte buffer, so it must flush mid-value.
    let big = [Num::MIN; 12];
    assert!(postcard::to_allocvec(&big).unwrap().len() > 64);
    assert_writer_matches(&big);
    assert_writer_matches(&(7_u8, Num::ONE, "name"));
    assert_writer_matches(&());
}

#[test]
#[should_panic(expected = "state type \"test.health\" registered twice")]
fn duplicate_names_are_refused() {
    let mut hasher = hasher();
    hasher.register_component::<Health>();
}
