use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

/// A registry of callbacks, each under a key, through which a lower module calls code of the
/// modules above it. A module that calls the registry declares so, and each module above declares
/// its callbacks, so the installer puts in the registry and registers them. Not state.
pub trait Callbacks: Resource + Component<Mutability = Mutable> + Default {
    type Key: Copy + Eq + Debug + Send + Sync + 'static;
    type Callback: Copy + Send + Sync + 'static;

    /// The capability whose module declares the callback of `key`; none for the core's.
    fn capability(key: Self::Key) -> Option<Capability>;

    /// Registers `callback` under `key`, which has none yet.
    fn register(&mut self, key: Self::Key, callback: Self::Callback);
}
