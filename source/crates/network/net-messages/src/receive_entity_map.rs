use bevy_derive::Deref;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity::hash_map::EntityHashMap;
use bevy_reflect::Reflect;

#[derive(Default, Debug, Reflect, Deref)]
pub struct ReceiveEntityMap(pub(crate) EntityHashMap<Entity>);

impl ReceiveEntityMap {
    /// Maps `remote` to `local`.
    pub fn insert(&mut self, remote: Entity, local: Entity) {
        self.0.insert(remote, local);
    }
}
