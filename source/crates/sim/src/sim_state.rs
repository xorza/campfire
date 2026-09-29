use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A component that is part of the simulated state: hashed, snapshotted and restored. `NAME` fixes
/// its place in the hash, so it never changes while the component keeps its meaning.
pub trait SimComponent: Component + Serialize + DeserializeOwned {
    const NAME: &'static str;
}

/// A resource that is part of the simulated state.
pub trait SimResource: Resource + Serialize + DeserializeOwned {
    const NAME: &'static str;
}
