use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use serde::Serialize;

/// A component that is part of the simulated state: hashed, and later snapshotted. `NAME` fixes
/// its place in the hash, so it never changes while the component keeps its meaning.
pub trait SimComponent: Component + Serialize {
    const NAME: &'static str;
}

/// A resource that is part of the simulated state.
pub trait SimResource: Resource + Serialize {
    const NAME: &'static str;
}
