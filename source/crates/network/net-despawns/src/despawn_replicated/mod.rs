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
mod tests;
