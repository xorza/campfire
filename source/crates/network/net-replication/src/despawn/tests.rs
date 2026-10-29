use bevy_replicon::prelude::RepliconTick;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_replicon::shared::replication::registry::test_fns::TestFnsEntityExt;
use campfire_net_core::NetTick;

use super::*;

/// A world whose confirmed tick 1 maps to tick 10, under `policy`.
fn world(policy: InterpolatedDespawn) -> World {
    let mut world = World::new();
    let mut confirmed_ticks = ConfirmedTickMap::default();
    confirmed_ticks.record(RepliconTick::new(1), NetTick(10));
    world.insert_resource(confirmed_ticks);
    world.insert_resource(policy);
    world.init_resource::<ReplicationRegistry>();
    world.resource_mut::<ReplicationRegistry>().despawn = receive;
    world
}

fn deliver(world: &mut World, entity: Entity, message_tick: u32) {
    world
        .entity_mut(entity)
        .apply_despawn(RepliconTick::new(message_tick));
}

/// Under the delay, an interpolated entity waits at the tick its confirmed tick maps, 10; one whose
/// confirmed tick no map holds is despawned at once and counted. Without the delay, or for an
/// entity that is not interpolated, the despawn is immediate.
#[test]
fn only_a_mapped_interpolated_despawn_waits_under_the_delay() {
    let mut world = world(InterpolatedDespawn::AtInterpolationTick);
    let waits = world.spawn(Interpolated).id();
    let unmapped = world.spawn(Interpolated).id();
    let predicted = world.spawn_empty().id();
    deliver(&mut world, waits, 1);
    deliver(&mut world, unmapped, 2);
    deliver(&mut world, predicted, 1);

    assert_eq!(
        world.get::<DelayedDespawn>(waits),
        Some(&DelayedDespawn::new(NetTick(10)))
    );
    assert!(world.get_entity(unmapped).is_err());
    assert!(world.get_entity(predicted).is_err());
    assert_eq!(
        world
            .resource::<ConfirmedTickMap>()
            .faults()
            .unmapped_messages,
        1
    );

    let mut world = self::world(InterpolatedDespawn::Immediate);
    let interpolated = world.spawn(Interpolated).id();
    deliver(&mut world, interpolated, 1);
    assert!(world.get_entity(interpolated).is_err());
}
