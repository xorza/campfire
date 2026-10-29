use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use campfire_net_despawns::DespawnReplicated;

use super::*;

#[derive(Component, Debug, PartialEq)]
struct A(u32);

#[derive(Component, Debug, Default, PartialEq)]
struct B;

/// Counts its drops.
#[derive(Component)]
struct Counted(Arc<AtomicUsize>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Component, Debug, PartialEq)]
struct C;

/// The changes of each entity apply together, whatever the order of the entities: the first
/// gains `A(1)` and `B` and loses its `C`, and the second gains `A(2)`.
#[test]
fn each_entity_takes_its_changes_together() {
    let mut world = World::new();
    let first = world.spawn(C).id();
    let second = world.spawn_empty().id();
    let mut deferred = DeferredEntityCommands::default();
    deferred.insert(first, A(1));
    deferred.insert(second, A(2));
    deferred.insert(first, B);
    deferred.remove::<C>(first);

    deferred.apply_to(&mut world);

    assert_eq!(world.get::<A>(first), Some(&A(1)));
    assert_eq!(world.get::<B>(first), Some(&B));
    assert_eq!(world.get::<C>(first), None);
    assert_eq!(world.get::<A>(second), Some(&A(2)));
    assert_eq!(world.get::<B>(second), None);
}

/// A despawned entity's components are dropped, once, and so are those of a buffer dropped
/// before it applies.
#[test]
fn a_change_that_does_not_apply_drops_its_component_once() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut world = World::new();
    let gone = world.spawn_empty().id();
    DespawnReplicated.apply(world.entity_mut(gone));
    let mut deferred = DeferredEntityCommands::default();
    deferred.insert(gone, Counted(Arc::clone(&drops)));
    deferred.apply_to(&mut world);
    assert_eq!(drops.load(Ordering::Relaxed), 1);

    let alive = world.spawn_empty().id();
    deferred.insert(alive, Counted(Arc::clone(&drops)));
    drop(deferred);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    assert!(world.get::<Counted>(alive).is_none());
}

/// Applying keeps the buffers' capacity, so the next frame queues without allocating.
#[test]
fn applying_keeps_the_buffers() {
    let mut world = World::new();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let mut deferred = DeferredEntityCommands::default();
    deferred.insert(first, A(1));
    deferred.insert(second, A(2));
    let ops = deferred.ops.capacity();
    let values = deferred.stores[0].capacity();

    deferred.apply_to(&mut world);

    assert_eq!(world.get::<A>(second), Some(&A(2)));
    assert!(deferred.ops.is_empty());
    assert_eq!(deferred.ops.capacity(), ops);
    assert_eq!(deferred.stores[0].capacity(), values);
}

#[derive(Resource)]
struct Target(Entity);

fn insert_a(target: Res<'_, Target>, mut deferred: Deferred<'_, DeferredEntityCommands>) {
    deferred.insert(target.0, A(7));
}

/// A system's buffer applies when the system's deferred changes do.
#[test]
fn a_system_buffer_applies_after_its_system() {
    let mut app = App::new();
    let entity = app.world_mut().spawn_empty().id();
    app.insert_resource(Target(entity));
    app.add_systems(Update, insert_a);
    app.update();
    assert_eq!(app.world().get::<A>(entity), Some(&A(7)));
}

#[derive(Component)]
struct Joined;

fn insert_a_on_join(
    trigger: On<'_, '_, Add<Joined>>,
    mut deferred: Deferred<'_, DeferredEntityCommands>,
) {
    deferred.insert(trigger.entity, A(9));
}

/// An observer's buffers come back as a spare once its command applies, and its runs take turns
/// with that spare: the observer starts with buffer 0 and gets buffer 1 at its first trigger, so
/// the third trigger fills buffer 0 again, with no allocation, and the spare it leaves has the ops
/// of the first. Each entity has its `A`.
#[test]
fn an_observer_reuses_the_buffers_its_last_command_applied() {
    let mut world = World::new();
    world.add_observer(insert_a_on_join);
    let mut spare_ops = Vec::new();
    for _ in 0..3 {
        let entity = world.spawn(Joined).id();
        world.flush();

        assert_eq!(world.get::<A>(entity), Some(&A(9)));
        let mut spares = world.resource_mut::<SpareBuffers>();
        let spares = spares.0.get();
        assert_eq!(spares.len(), 1);
        assert!(spares[0].ops.is_empty());
        assert_eq!(spares[0].stores.len(), 1);
        spare_ops.push(spares[0].ops.as_ptr());
    }
    assert_eq!(spare_ops[2], spare_ops[0]);
    assert_ne!(spare_ops[1], spare_ops[0]);
}
