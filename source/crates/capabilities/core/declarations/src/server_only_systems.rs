use bevy_ecs::schedule::SystemSet;

/// The systems that compute the server's own state, which `InstallCx::server` adds to a server's
/// match alone: a client's install leaves them out, as that state reaches it from the server
/// (10-replication-audiences.md, Prediction reads only what its client holds). Each also sits in a
/// set of its own module, which places it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerOnlySystems;
