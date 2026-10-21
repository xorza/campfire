use super::*;

/// A parent with two `Remote` children, a local one between them, a `Remote` grandchild under
/// the first `Remote` child, and a `Remote` grandchild under the local child.
#[derive(Debug)]
struct Family {
    parent: Entity,
    remotes: [Entity; 2],
    local: Entity,
    grandchild: Entity,
    held: Entity,
}

impl Family {
    fn spawn(world: &mut World) -> Self {
        let parent = world.spawn(Remote).id();
        let first = world.spawn((Remote, ChildOf(parent))).id();
        let local = world.spawn(ChildOf(parent)).id();
        let second = world.spawn((Remote, ChildOf(parent))).id();
        let grandchild = world.spawn((Remote, ChildOf(first))).id();
        let held = world.spawn((Remote, ChildOf(local))).id();
        Self {
            parent,
            remotes: [first, second],
            local,
            grandchild,
            held,
        }
    }
}

/// The despawn detaches each `Remote` entity that local entities hold, which keeps its other
/// components, and takes each local child with the entity; a grandchild under a `Remote` child
/// stays attached to it. Bevy's own despawn takes every child, as the server's children are no
/// different to it.
#[test]
fn a_replicated_despawn_detaches_the_remote_children_only() {
    let mut world = World::new();
    let family = Family::spawn(&mut world);
    DespawnReplicated.apply(world.entity_mut(family.parent));
    world.flush();
    assert!(world.get_entity(family.parent).is_err());
    assert!(world.get_entity(family.local).is_err());
    for remote in family.remotes.into_iter().chain([family.held]) {
        let remote = world.entity(remote);
        assert!(remote.contains::<Remote>() && !remote.contains::<ChildOf>());
    }
    assert_eq!(
        world.get::<ChildOf>(family.grandchild).map(ChildOf::parent),
        Some(family.remotes[0])
    );

    let mut world = World::new();
    let family = Family::spawn(&mut world);
    world.entity_mut(family.parent).despawn();
    world.flush();
    for gone in [family.local, family.grandchild, family.held]
        .into_iter()
        .chain(family.remotes)
    {
        assert!(world.get_entity(gone).is_err());
    }
}
