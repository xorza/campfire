use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use campfire_math::{Num, SegmentSeed, Vec3};
use campfire_sim::{EntityIndex, IdAllocator, SimUpdate, StableId, TickRate, TypeHash};

use super::*;
use crate::navigation::lane_walker::PathDirection;
use crate::units::lane::Lane;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const ONE: i64 = 1 << 24;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

#[derive(Debug)]
struct Walk {
    world: World,
    registry: StateRegistry,
}

impl Walk {
    fn new() -> Walk {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), RATE);
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        Navigation::install(&mut world, &mut schedule, &mut registry);
        world.add_schedule(schedule);
        Walk { world, registry }
    }

    /// A unit at `at` walking a meter a tick to `to`.
    fn unit(&mut self, at: Position, to: Option<Position>) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let entity = self
            .world
            .spawn((id, at, MoveStep::new(Num::ONE).unwrap().bundle()))
            .id();
        self.world.get_mut::<Destination>(entity).unwrap().set(to);
        id
    }

    fn get<C: Component + Copy>(&self, id: StableId) -> C {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        *self.world.entity(entity).get::<C>().unwrap()
    }

    fn tick(&mut self) {
        self.world.run_schedule(SimUpdate);
    }
}

#[test]
fn a_unit_walks_to_its_destination_exactly() {
    let mut walk = Walk::new();
    let straight = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    // Offset (3, 0, 4), distance 5: the first meter is 3/5 and 4/5, each rounded once:
    // 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The height stays at y = 2.
    let slanted = walk.unit(at(4, 2, 0), Some(at(7, 2, 4)));
    let still = walk.unit(at(9, 0, 9), None);

    walk.tick();
    assert_eq!(walk.get::<Position>(straight), at(0, 0, 1));
    assert_eq!(
        walk.get::<Position>(slanted),
        Position::new(Vec3::new(
            Num::from_bits(4 * ONE + 10_066_330),
            num(2),
            Num::from_bits(13_421_773)
        ))
        .unwrap()
    );
    assert_eq!(walk.get::<Position>(still), at(9, 0, 9));
    // Straight along z, one meter a tick: done in the fifth tick, and the destination dropped.
    for z in 2..=5 {
        walk.tick();
        assert_eq!(walk.get::<Position>(straight), at(0, 0, z));
    }
    assert_eq!(walk.get::<Destination>(straight).get(), None);
    walk.tick();
    assert_eq!(walk.get::<Position>(straight), at(0, 0, 5));
}

#[test]
fn a_dead_unit_stays_and_forgets_its_destination() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
    walk.world.entity_mut(entity).insert(Dead);
    walk.tick();
    assert_eq!(walk.get::<Position>(unit), at(0, 0, 0));
    assert_eq!(walk.get::<Destination>(unit).get(), None);
}

#[test]
fn lanes_count_waypoints_in_either_direction() {
    let lanes = Lanes::new([
        ("near", &[at(0, 0, 0), at(1, 0, 0)][..]),
        ("far", &[at(5, 0, 5)][..]),
    ]);
    assert_eq!(lanes.count(), 2);
    assert_eq!(
        (lanes.named("far"), lanes.named("none")),
        (Some(Lane::new(1)), None)
    );
    assert_eq!(
        (lanes.name(Lane::new(0)), lanes.name(Lane::new(1))),
        ("near", "far")
    );
    let walk = |lane, direction| {
        (0..3)
            .map(|index| lanes.waypoint(lane, index, direction))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        walk(Lane::new(0), PathDirection::Forward),
        [Some(at(0, 0, 0)), Some(at(1, 0, 0)), None]
    );
    assert_eq!(
        walk(Lane::new(0), PathDirection::Backward),
        [Some(at(1, 0, 0)), Some(at(0, 0, 0)), None]
    );
    assert_eq!(
        walk(Lane::new(1), PathDirection::Backward),
        [Some(at(5, 0, 5)), None, None]
    );
    assert_eq!(
        walk(Lane::new(2), PathDirection::Forward),
        [None, None, None]
    );
}

#[test]
fn every_navigation_type_is_state() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
    walk.world.entity_mut(entity).insert((
        LaneWalker::start(PathDirection::Forward),
        OnLane::new(Lane::new(0)),
    ));
    let registry = &walk.registry;
    let mut per_type = Vec::new();
    let hash = registry.hash_by_type(&walk.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    assert_eq!(
        names,
        [
            "navigation.destination",
            "navigation.lane_walker",
            "navigation.move_step",
            "navigation.on_lane",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
        ]
    );
    let mut snapshot = Vec::new();
    registry.snapshot(&walk.world, &mut snapshot);
    let mut restored = Walk::new();
    registry.restore(&snapshot, &mut restored.world).unwrap();
    assert_eq!(registry.hash(&restored.world), hash);
}

#[test]
fn move_steps_are_never_negative() {
    assert_eq!(MoveStep::new(-Num::EPSILON), None);
    assert_eq!(MoveStep::new(Num::ZERO).map(MoveStep::get), Some(Num::ZERO));
    let negative = postcard::to_allocvec(&-Num::EPSILON).unwrap();
    assert!(postcard::from_bytes::<MoveStep>(&negative).is_err());
    let one = postcard::to_allocvec(&Num::ONE).unwrap();
    assert_eq!(
        postcard::from_bytes::<MoveStep>(&one).ok(),
        MoveStep::new(Num::ONE)
    );
}
