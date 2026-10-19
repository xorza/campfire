use bevy_derive::Deref;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity::hash_map::EntityHashMap;
use bevy_reflect::Reflect;

#[derive(Default, Debug, Reflect, Deref)]
pub struct SendEntityMap(pub(crate) EntityHashMap<Entity>);

impl SendEntityMap {
    /// Maps `local` to `remote`.
    pub fn insert(&mut self, local: Entity, remote: Entity) {
        self.0.insert(local, remote);
    }
}
