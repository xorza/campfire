use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_math::{Num, Vec3};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
struct Health(Num);

impl SimComponent for Health {
    const NAME: &'static str = "test.health";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
struct Position(Vec3);

impl SimComponent for Position {
    const NAME: &'static str = "test.position";

    /// The tests' rule, which reads another type: a unit that stands somewhere has health.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<Health>(entity).is_some()
    }
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

fn registry() -> StateRegistry {
    let mut registry = StateRegistry::new();
    registry.register_component::<Position>();
    registry.register_component::<Health>();
    registry
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
    registry().hash_by_type(world, &mut per_type);
    per_type
}

fn changed_types(before: &[TypeHash], after: &[TypeHash]) -> Vec<&'static str> {
    before
        .iter()
        .zip(after)
        .filter(|(b, a)| b.hash != a.hash)
        .map(|(b, _)| b.name)
        .collect()
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

/// A despawned id, an entity with no registered component, and extreme values.
fn varied_world() -> World {
    let mut world = plain_world();
    let gone = allocate(&mut world);
    let gone = world.spawn((gone, health(5))).id();
    world.despawn(gone);
    let bare = allocate(&mut world);
    world.spawn((bare, Replicated));
    let extreme = allocate(&mut world);
    world.spawn((
        extreme,
        Health(Num::MIN),
        Position(Vec3::new(Num::MAX, Num::MIN, Num::EPSILON)),
    ));
    world
}

fn snapshot(world: &World) -> Vec<u8> {
    let mut bytes = Vec::new();
    registry().snapshot(world, &mut bytes);
    bytes
}

fn restore(bytes: &[u8]) -> Result<World, SnapshotError> {
    let mut world = World::new();
    registry().restore(bytes, &mut world)?;
    Ok(world)
}

/// A snapshot framed by hand from (name, body) pairs.
fn frame(sections: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = SNAPSHOT_TAG.to_vec();
    for (name, body) in sections {
        bytes.extend_from_slice(&u32::try_from(name.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(&u64::try_from(body.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(body);
    }
    bytes
}

fn encoded<T: Serialize>(values: &[T]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| postcard::to_allocvec(value).unwrap())
        .collect()
}

#[test]
fn types_hash_in_name_order() {
    let names: Vec<_> = by_type(&plain_world()).iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
            "test.health",
            "test.position"
        ]
    );
}

#[test]
fn build_order_does_not_matter() {
    let registry = registry();
    assert_eq!(
        registry.hash(&plain_world()),
        registry.hash(&shuffled_world())
    );
    assert_eq!(snapshot(&plain_world()), snapshot(&shuffled_world()));
    // A hash writes as its 32 bytes in lowercase hex, two digits each, leading zeros kept, and
    // reads back from that spelling only.
    let mut bytes = [0; 32];
    bytes[0] = 0x0a;
    bytes[31] = 0xff;
    let written = StateHash(Bytes32::new(bytes)).to_string();
    assert_eq!(written, format!("0a{}ff", "00".repeat(30)));
    assert_eq!(written.parse(), Ok(StateHash(Bytes32::new(bytes))));
    assert_eq!(written.to_uppercase().parse::<StateHash>(), Err(NotHex));
}

#[test]
fn one_changed_field_changes_only_its_type() {
    let mut world = shuffled_world();
    let before = by_type(&world);
    let entity = world.resource::<EntityIndex>().iter().next().unwrap().1;
    world.get_mut::<Health>(entity).unwrap().0 += Num::EPSILON;
    assert_eq!(changed_types(&before, &by_type(&world)), ["test.health"]);
}

#[test]
fn non_sim_components_change_nothing() {
    let mut world = plain_world();
    let before = registry().hash(&world);
    let entities: Vec<_> = world
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, e)| e)
        .collect();
    for entity in entities {
        world.entity_mut(entity).insert(Replicated);
    }
    world.spawn(Replicated);
    assert_eq!(registry().hash(&world), before);
}

#[test]
fn allocator_and_entity_list_are_hashed() {
    let mut world = plain_world();
    let before = by_type(&world);
    allocate(&mut world);
    assert_eq!(
        changed_types(&before, &by_type(&world)),
        ["sim.id_allocator"]
    );

    let before = by_type(&world);
    let bare = allocate(&mut world);
    world.spawn(bare);
    assert_eq!(
        changed_types(&before, &by_type(&world)),
        ["sim.entities", "sim.id_allocator"]
    );
}

#[test]
fn a_digest_is_blake3_over_its_domain_and_its_bytes() {
    let mut hasher = Hasher::new();
    hasher.update(b"campfire/digest/v1").update(b"abc");
    assert_eq!(
        StateHash::of(b"abc").as_bytes(),
        hasher.finalize().as_bytes()
    );
    assert_ne!(StateHash::of(b""), registry().hash(&varied_world()));
}

#[test]
fn snapshot_restores_the_same_state() {
    let mut original = varied_world();
    let bytes = snapshot(&original);
    let mut restored = restore(&bytes).unwrap();
    assert_eq!(registry().hash(&restored), registry().hash(&original));
    assert_eq!(snapshot(&restored), bytes);
    // The allocator continues where the original would.
    assert_eq!(allocate(&mut restored), allocate(&mut original));
}

#[test]
fn every_truncation_is_refused() {
    let bytes = snapshot(&varied_world());
    for len in 0..bytes.len() {
        assert!(restore(&bytes[..len]).is_err(), "truncated to {len} bytes");
    }
}

#[test]
fn corruption_never_panics_and_what_restores_snapshots_to_the_same_bytes() {
    let bytes = snapshot(&varied_world());
    for at in 0..bytes.len() {
        for flip in [0x01, 0x80, 0xFF] {
            let mut corrupt = bytes.clone();
            corrupt[at] ^= flip;
            // A flip that restores must be a state of its own: no two byte strings restore to one
            // state.
            if let Ok(world) = restore(&corrupt) {
                assert_eq!(
                    snapshot(&world),
                    corrupt,
                    "byte {at} flipped by {flip:#04x}"
                );
            }
        }
    }
}

#[test]
fn flawed_snapshots_are_refused() {
    let names = [
        "sim.entities",
        "sim.id_allocator",
        "test.health",
        "test.position",
    ];
    // Sim positions and the tick play no part in these flaws: every case has none and tick 0.
    let with = |bodies: [Vec<u8>; 4]| -> Vec<u8> {
        let mut sections: Vec<_> = names.iter().copied().zip(bodies).collect();
        sections.insert(2, ("sim.position", Vec::new()));
        sections.insert(
            3,
            ("sim.tick", postcard::to_allocvec(&Some(0_u64)).unwrap()),
        );
        frame(&sections)
    };
    let allocator = |next: u64| postcard::to_allocvec(&Some(next)).unwrap();
    let valid = with([encoded(&[0_u64, 1]), allocator(2), Vec::new(), Vec::new()]);
    assert!(restore(&valid).is_ok());

    let cases = [
        (b"not a snapshot".to_vec(), SnapshotError::NotSnapshot),
        ([valid.as_slice(), &[0]].concat(), SnapshotError::Trailing),
        (valid[..valid.len() - 1].to_vec(), SnapshotError::Truncated),
        (
            frame(&[("sim.entities", Vec::new())]),
            SnapshotError::TypesDiffer,
        ),
        (
            with([encoded(&[1_u64, 0]), allocator(2), Vec::new(), Vec::new()]),
            SnapshotError::NotCanonical,
        ),
        (
            with([
                encoded(&[0_u64]),
                allocator(2),
                encoded(&[(1_u64, health(1))]),
                Vec::new(),
            ]),
            SnapshotError::UnknownEntity,
        ),
        (
            with([
                encoded(&[0_u64]),
                allocator(1),
                Vec::new(),
                encoded(&[(0_u64, position(1))]),
            ]),
            SnapshotError::Invalid("test.position"),
        ),
        (
            with([encoded(&[0_u64, 1]), allocator(1), Vec::new(), Vec::new()]),
            SnapshotError::AllocatorBehind,
        ),
        (
            with([
                Vec::new(),
                [allocator(2), vec![0]].concat(),
                Vec::new(),
                Vec::new(),
            ]),
            SnapshotError::Trailing,
        ),
        (
            with([vec![0xFF], allocator(2), Vec::new(), Vec::new()]),
            // One byte with the continuation bit set: the varint of an id ends early.
            SnapshotError::Malformed(postcard::Error::DeserializeUnexpectedEnd),
        ),
    ];
    for (bytes, expected) in cases {
        assert_eq!(
            restore(&bytes).err(),
            Some(expected.clone()),
            "{expected:?}"
        );
    }

    // "test.extra" sorts between the others; "zz.last" sorts after all of them.
    for extra in [Extra::register as fn(&mut StateRegistry), Last::register] {
        let mut wider = registry();
        extra(&mut wider);
        let mut world = World::new();
        assert_eq!(
            wider.restore(&valid, &mut world).err(),
            Some(SnapshotError::TypesDiffer)
        );
        let mut wider_snapshot = Vec::new();
        wider.snapshot(&new_world(), &mut wider_snapshot);
        assert_eq!(
            restore(&wider_snapshot).err(),
            Some(SnapshotError::TypesDiffer)
        );
    }
}

#[test]
fn a_resource_the_snapshot_lacks_is_removed() {
    // A world without `Extra` snapshots it as absent; a world that holds one loses it on restore,
    // so its hash is the snapshot's.
    let mut with_extra = registry();
    Extra::register(&mut with_extra);
    let mut bytes = Vec::new();
    with_extra.snapshot(&plain_world(), &mut bytes);
    let mut world = World::new();
    world.insert_resource(Extra);
    with_extra.restore(&bytes, &mut world).unwrap();
    assert!(world.get_resource::<Extra>().is_none());
    assert_eq!(with_extra.hash(&world), with_extra.hash(&plain_world()));
}

#[derive(Resource, Debug, Serialize, Deserialize)]
struct Extra;

impl SimResource for Extra {
    const NAME: &'static str = "test.extra";

    fn check(&self, _: &World) -> bool {
        true
    }
}

#[derive(Resource, Debug, Serialize, Deserialize)]
struct Last;

impl SimResource for Last {
    const NAME: &'static str = "zz.last";

    fn check(&self, _: &World) -> bool {
        true
    }
}

trait Register {
    fn register(registry: &mut StateRegistry);
}

impl<R: SimResource> Register for R {
    fn register(registry: &mut StateRegistry) {
        registry.register_resource::<R>();
    }
}

fn assert_writer_matches<T: Serialize>(value: &T) {
    let bytes = postcard::to_allocvec(value).unwrap();
    let mut expected = Hasher::new();
    expected.update(&bytes);
    let mut actual = Hasher::new();
    write(&mut actual, value);
    assert_eq!(actual.finalize(), expected.finalize());
    let mut written = Vec::new();
    write(&mut written, value);
    assert_eq!(written, bytes);
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
    let mut registry = registry();
    registry.register_component::<Health>();
}

#[test]
#[should_panic(expected = "restore needs a world without sim entities")]
fn restore_refuses_a_populated_world() {
    let mut world = plain_world();
    let _outcome = registry().restore(&snapshot(&plain_world()), &mut world);
}
