use bevy_ecs::prelude::*;
use bevy_replicon::prelude::Remote;

/// Despawns an entity, where the server holds each replicated entity's life. Each [`Remote`] entity
/// under it that only local entities hold first loses its [`ChildOf`], the view of the server's
/// relation, and keeps what the server sent: the server ends that entity's life with a despawn of
/// its own, as it does for each entity it despawns or hides. The local entities under it die with
/// it, as Bevy's [`ChildOf`] makes them. An entity with no `Remote` entity under it, as each on a
/// server, despawns as Bevy despawns it (11-net-stack/replication.md, Every despawn of the
/// replication crates is `DespawnReplicated`, in a proxy crate of its own).
#[derive(Debug, Clone, Copy, Default)]
pub struct DespawnReplicated;

impl EntityCommand for DespawnReplicated {
    type Out = ();

    fn apply(self, mut entity: EntityWorldMut<'_>) {
        let id = entity.id();
        entity.world_scope(|world| detach_remotes(world, id));
        entity.despawn();
    }
}

/// Detaches each `Remote` child of `parent`, and those under each local child, which Bevy's
/// despawn of `parent` takes; a `Remote` child keeps its own children.
fn detach_remotes(world: &mut World, parent: Entity) {
    let mut index = 0;
    while let Some(&child) = world
        .get::<Children>(parent)
        .and_then(|children| children.get(index))
    {
        if world.entity(child).contains::<Remote>() {
            world.entity_mut(child).remove::<ChildOf>();
        } else {
            detach_remotes(world, child);
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
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
}
