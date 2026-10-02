use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A component that is part of the simulated state: hashed, snapshotted and restored. `NAME` fixes
/// its place in the hash, so it never changes while the component keeps its meaning.
pub trait SimComponent: Component + Serialize + DeserializeOwned {
    const NAME: &'static str;

    /// Whether the value, restored on `entity`, keeps its type's rules in `world`, with every
    /// other type restored and the match's books in place: each id it holds names what the books
    /// hold, and each shape the packages fix matches theirs. A snapshot is untrusted, so a value
    /// that breaks a rule fails the restore, and nothing later panics on it.
    fn check(&self, world: &World, entity: Entity) -> bool;
}

/// A resource that is part of the simulated state.
pub trait SimResource: Resource + Serialize + DeserializeOwned {
    const NAME: &'static str;

    /// Whether the value, restored, keeps its type's rules in `world`, as `SimComponent::check`
    /// says.
    fn check(&self, world: &World) -> bool;
}
