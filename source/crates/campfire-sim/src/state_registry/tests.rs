use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_common::Tick;
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
    world.insert_resource(SimTick::new(Tick::new(7)));
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
    bytes.extend_from_slice(&StateRegistry::DATA_VERSION.to_le_bytes());
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
        StateRegistry::digest(b"abc").as_bytes(),
        hasher.finalize().as_bytes()
    );
    assert_ne!(StateRegistry::digest(b""), registry().hash(&varied_world()));
}

#[test]
fn snapshot_restores_the_same_state() {
    let mut original = varied_world();
    let bytes = snapshot(&original);
    // A snapshot's state hash is the one its state hashes to.
    assert_eq!(
        registry().snapshot(&original, &mut Vec::new()),
        registry().hash(&original)
    );
    // The data version follows the tag, four bytes little-endian; one past this release's is
    // another release's format, refused with both numbers before any type is read, and a
    // snapshot that ends at its tag is cut short.
    let version = SNAPSHOT_TAG.len()..SNAPSHOT_TAG.len() + 4;
    assert_eq!(
        bytes[version.clone()],
        StateRegistry::DATA_VERSION.to_le_bytes()
    );
    let mut other = bytes.clone();
    other[version].copy_from_slice(&(StateRegistry::DATA_VERSION + 1).to_le_bytes());
    assert_eq!(
        restore(&other).err(),
        Some(SnapshotError::OtherVersion {
            found: StateRegistry::DATA_VERSION + 1,
            read: StateRegistry::DATA_VERSION,
        })
    );
    assert_eq!(restore(SNAPSHOT_TAG).err(), Some(SnapshotError::Truncated));
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
    let at_tick = |bodies: [Vec<u8>; 4], tick: Option<u64>| -> Vec<u8> {
        let mut sections: Vec<_> = names.iter().copied().zip(bodies).collect();
        sections.insert(2, ("sim.position", Vec::new()));
        sections.insert(3, ("sim.tick", postcard::to_allocvec(&tick).unwrap()));
        frame(&sections)
    };
    let with = |bodies| at_tick(bodies, Some(0));
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
        // The sim's own resources, recorded absent.
        (
            at_tick([Vec::new(), allocator(0), Vec::new(), Vec::new()], None),
            SnapshotError::Missing("sim.tick"),
        ),
        (
            with([Vec::new(), vec![0], Vec::new(), Vec::new()]),
            SnapshotError::Missing("sim.id_allocator"),
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
    let mut plain = plain_world();
    plain.init_resource::<SimTick>();
    let mut bytes = Vec::new();
    with_extra.snapshot(&plain, &mut bytes);
    let mut world = World::new();
    world.insert_resource(Extra);
    with_extra.restore(&bytes, &mut world).unwrap();
    assert!(world.get_resource::<Extra>().is_none());
    assert_eq!(with_extra.hash(&world), with_extra.hash(&plain));
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
    let mut actual = HashSink::new();
    Writer::write(&mut actual, value);
    assert_eq!(actual.finish(), *expected.finalize().as_bytes());
    let mut written = Vec::new();
    Writer::write(&mut written, value);
    assert_eq!(written, bytes);
    // Three in one writer, as a type's entities: the three encodings one after another.
    let mut each = Vec::new();
    Writer::write_each(&mut each, [value; 3]);
    assert_eq!(each, bytes.repeat(3));
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

/// A section of a delta that holds bytes: its type's name and its bytes.
type Section = (&'static str, Vec<u8>);

/// The sections of `delta` that hold bytes, by type name, in the registry's order.
fn sections(registry: &StateRegistry, delta: &StateDelta) -> Vec<Section> {
    (0..delta.sections())
        .filter(|&at| !delta.section(at).is_empty())
        .map(|at| (registry.entries[at].name, delta.section(at).to_vec()))
        .collect()
}

/// A world's state and a copy of it that follows by deltas.
#[derive(Debug)]
struct Following {
    registry: StateRegistry,
    world: World,
    copy: World,
    delta: StateDelta,
}

impl Following {
    fn new(registry: StateRegistry, world: World) -> Following {
        let mut following = Following {
            registry,
            world,
            copy: World::new(),
            delta: StateDelta::default(),
        };
        following.copy.init_resource::<EntityIndex>();
        following
            .registry
            .track(&mut following.world, &mut following.delta);
        following
            .registry
            .apply(&following.delta, &mut following.copy);
        following
    }

    /// Changes the world by `change`, copies what changed, and checks that the copy's state is
    /// the world's; the delta's lost and gained ids, and its sections that hold bytes.
    fn step(
        &mut self,
        change: impl FnOnce(&mut World),
    ) -> (Vec<StableId>, Vec<StableId>, Vec<Section>) {
        change(&mut self.world);
        self.registry.changes(&mut self.world, &mut self.delta);
        self.registry.apply(&self.delta, &mut self.copy);
        assert_eq!(self.by_type(&self.copy), self.by_type(&self.world));
        (
            self.delta.lost.clone(),
            self.delta.gained.clone(),
            sections(&self.registry, &self.delta),
        )
    }

    fn by_type(&self, world: &World) -> Vec<TypeHash> {
        let mut per_type = Vec::new();
        self.registry.hash_by_type(world, &mut per_type);
        per_type
    }
}

fn entity(world: &World, id: StableId) -> Entity {
    world.resource::<EntityIndex>().get(id).unwrap()
}

#[test]
fn a_copy_follows_each_kind_of_change_by_the_values_that_changed() {
    // A world whose queries skip the unpredicted, as a predicting client's do.
    let mut world = plain_world();
    Unpredicted::register(&mut world);
    let mut following = Following::new(registry(), world);
    let (first, second) = (StableId::new(0), StableId::new(1));
    assert_eq!(by_type(&following.copy), by_type(&following.world));
    // No tick resource: each delta holds its absence, `None`, a 0 byte.
    let absent = ("sim.tick", vec![0]);
    let allocator = |world: &World| {
        (
            "sim.id_allocator",
            encoded(&[Some(world.resource::<IdAllocator>())]),
        )
    };

    // Nothing changed: nothing but the absent tick.
    assert_eq!(
        following.step(|_| {}),
        (vec![], vec![], vec![absent.clone()])
    );

    // One value changed: that value alone.
    let changed = following.step(|world| {
        world.get_mut::<Health>(entity(world, first)).unwrap().0 = Num::from_int(11).unwrap();
    });
    let health_11 = ("test.health", encoded(&[(first, Some(health(11)))]));
    assert_eq!(changed, (vec![], vec![], vec![absent.clone(), health_11]));

    // A new entity: its id, the allocator, and each of its values.
    let third = StableId::new(2);
    let spawned = following.step(|world| {
        let id = allocate(world);
        world.spawn((id, health(30)));
    });
    let health_30 = ("test.health", encoded(&[(third, Some(health(30)))]));
    assert_eq!(
        spawned,
        (
            vec![],
            vec![third],
            vec![allocator(&following.world), absent.clone(), health_30]
        )
    );

    // A removed component, and a despawned entity: its id alone, as its removals go with it.
    let removed = following.step(|world| {
        world.entity_mut(entity(world, first)).remove::<Position>();
        world.despawn(entity(world, second));
    });
    let unplaced = ("test.position", encoded(&[(first, None::<Position>)]));
    assert_eq!(
        removed,
        (vec![second], vec![], vec![absent.clone(), unplaced])
    );

    // An entity spawned and despawned between two copies, and a component removed and put back:
    // the allocator, and the value put back.
    let back = following.step(|world| {
        let id = allocate(world);
        let gone = world.spawn((id, health(40))).id();
        world.despawn(gone);
        let third = entity(world, third);
        world.entity_mut(third).remove::<Health>();
        world.entity_mut(third).insert(health(31));
    });
    let health_31 = ("test.health", encoded(&[(third, Some(health(31)))]));
    assert_eq!(
        back,
        (
            vec![],
            vec![],
            vec![allocator(&following.world), absent.clone(), health_31]
        )
    );

    // An entity given another stable id: the old id lost, the new one gained with every value.
    let fifth = StableId::new(4);
    let renamed = following.step(|world| {
        let id = allocate(world);
        world.entity_mut(entity(world, third)).insert(id);
    });
    let moved = ("test.health", encoded(&[(fifth, Some(health(31)))]));
    assert_eq!(
        renamed,
        (
            vec![third],
            vec![fifth],
            vec![allocator(&following.world), absent.clone(), moved]
        )
    );

    // A value of an entity the world's default filters hide, as a client hides the units it
    // does not predict: copied all the same.
    let hidden = following.step(|world| {
        let fifth = entity(world, fifth);
        world.entity_mut(fifth).insert(Unpredicted);
        world.get_mut::<Health>(fifth).unwrap().0 = Num::from_int(32).unwrap();
    });
    let health_32 = ("test.health", encoded(&[(fifth, Some(health(32)))]));
    assert_eq!(hidden, (vec![], vec![], vec![absent.clone(), health_32]));

    // A resource inserted, then removed.
    let tick = SimTick::new(Tick::new(5));
    let inserted = following.step(|world| world.insert_resource(tick));
    let present = ("sim.tick", encoded(&[Some(tick)]));
    assert_eq!(inserted, (vec![], vec![], vec![present]));
    let gone = following.step(|world| {
        world.remove_resource::<SimTick>();
    });
    assert_eq!(gone, (vec![], vec![], vec![absent]));
}

#[test]
fn a_copy_takes_every_value_after_a_tick_check_finds_the_last_copy_too_old() {
    let mut following = Following::new(registry(), plain_world());
    let (first, second) = (StableId::new(0), StableId::new(1));
    let absent = ("sim.tick", vec![0]);

    // Bevy's check of the change ticks, at `present`, finds the last copy within the maximum
    // change age: it keeps its tick, and the next copy takes nothing new.
    let present = following.world.change_tick();
    following
        .world
        .resource_mut::<StateChanges>()
        .check_ticks(present);
    assert_eq!(
        following.step(|_| {}),
        (vec![], vec![], vec![absent.clone()])
    );

    // The check finds the last copy one tick older than the maximum age. The first's health
    // changed after it, but as long ago as that age, to which the check clamps the change's tick,
    // so it reads as no newer than the copy; the second's changed a tick ago. The next copy takes
    // every value, as a first does: the allocator and the first's position, which did not
    // change, among them.
    let present = following.world.change_tick();
    let ago = |ticks: u32| ChangeTick::new(present.get().wrapping_sub(ticks));
    let max_age = ChangeTick::MAX.get();
    let world = &mut following.world;
    let mut long_ago = world.get_mut::<Health>(entity(world, first)).unwrap();
    *long_ago = health(11);
    long_ago.set_last_changed(ago(max_age));
    *world.get_mut::<Health>(entity(world, second)).unwrap() = health(21);
    let mut recorded = world.resource_mut::<StateChanges>();
    recorded.set_since(ago(max_age + 1));
    recorded.check_ticks(present);
    let (_, _, sections) = following.step(|_| {});
    let allocator = encoded(&[Some(following.world.resource::<IdAllocator>())]);
    let healths = encoded(&[(first, Some(health(11))), (second, Some(health(21)))]);
    assert_eq!(
        sections,
        [
            ("sim.id_allocator", allocator),
            absent,
            ("test.health", healths),
            ("test.position", encoded(&[(first, Some(position(1)))])),
        ]
    );
}

/// Stands for a state component that requires another, as a unit's modifiers require their
/// clocks.
#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
#[require(Clock)]
struct Timed(u32);

#[derive(Component, Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Clock(u32);

impl SimComponent for Timed {
    const NAME: &'static str = "test.timed";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl SimComponent for Clock {
    const NAME: &'static str = "test.clock";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

#[test]
fn a_copy_keeps_a_required_component_removed_before_its_requirer_changes() {
    let mut registry = StateRegistry::new();
    registry.register_component::<Timed>();
    registry.register_component::<Clock>();
    let mut world = new_world();
    let id = allocate(&mut world);
    world.spawn((id, Timed(1)));
    let mut following = Following::new(registry, world);
    following.step(|world| {
        world.entity_mut(entity(world, id)).remove::<Clock>();
    });
    // The changed value alone: no removal of the clock, which an insert would have put back.
    let (_, _, sections) = following.step(|world| {
        world.get_mut::<Timed>(entity(world, id)).unwrap().0 = 2;
    });
    let timed = ("test.timed", encoded(&[(id, Some(Timed(2)))]));
    assert_eq!(sections, [("sim.tick", vec![0]), timed]);
    assert!(
        following
            .copy
            .get::<Clock>(entity(&following.copy, id))
            .is_none()
    );
}
