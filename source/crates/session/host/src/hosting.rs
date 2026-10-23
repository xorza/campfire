use bevy_ecs::resource::Resource;

/// Where a session's server runs: on another host, or on a thread of its one player's client,
/// the only server that saves and loads. Both ends hold it, each as it was set up.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hosting {
    Remote,
    Local,
}
