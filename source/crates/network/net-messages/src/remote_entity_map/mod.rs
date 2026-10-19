use crate::receive_entity_map::ReceiveEntityMap;
use crate::send_entity_map::SendEntityMap;
use bevy_ecs::entity::Entity;
use bevy_reflect::Reflect;

/// A bijection between local and remote entities. (used mostly on client because it's when we
/// receive entity updates)
#[derive(Default, Debug, Reflect)]
pub struct RemoteEntityMap {
    remote_to_local: ReceiveEntityMap,
    local_to_remote: SendEntityMap,
}

impl RemoteEntityMap {
    /// Maps `remote_entity` and `local_entity` to each other, and drops a pair either entity was
    /// in, so each side maps an entity once.
    pub fn insert(&mut self, remote_entity: Entity, local_entity: Entity) {
        if let Some(old_local) = self.remote_to_local.0.insert(remote_entity, local_entity)
            && old_local != local_entity
        {
            self.local_to_remote.0.remove(&old_local);
        }
        if let Some(old_remote) = self.local_to_remote.0.insert(local_entity, remote_entity)
            && old_remote != remote_entity
        {
            self.remote_to_local.0.remove(&old_remote);
        }
    }

    /// The receive side: local entities by remote entity.
    pub const fn remote_to_local(&self) -> &ReceiveEntityMap {
        &self.remote_to_local
    }

    /// The send side: remote entities by local entity.
    pub const fn local_to_remote(&self) -> &SendEntityMap {
        &self.local_to_remote
    }

    /// The local entity `remote_entity` maps to.
    pub fn get_local(&self, remote_entity: Entity) -> Option<Entity> {
        self.remote_to_local.get(&remote_entity).copied()
    }

    pub fn clear(&mut self) {
        self.local_to_remote.0.clear();
        self.remote_to_local.0.clear();
    }
}

#[cfg(test)]
mod tests;
