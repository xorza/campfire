use bevy_ecs::resource::Resource;
use campfire_script::ScriptPools;

/// What a match is to its host, which the match keeps as it is built.
#[derive(Resource, Debug, Clone)]
pub enum Role {
    /// A server's or a verifier's match, which computes the whole state and runs scripts from
    /// its pools.
    Server(ScriptPools),
    /// A server's match of the rules alone, which computes the whole state and runs no scripts,
    /// as a test or a bench of the capabilities builds.
    Rules,
    /// A client's match, which predicts its own player's units and runs no scripts: it builds no
    /// system that computes the server's own state, which reaches it from the server
    /// (14-replication-audiences.md, Prediction reads only what its client holds).
    Client,
}

impl Role {
    /// Whether the match runs scripts: a server's from its pools.
    pub const fn scripts(&self) -> bool {
        matches!(self, Self::Server(_))
    }

    /// Whether the match computes the whole state, as a server's does: every role's but a
    /// client's.
    pub const fn computes_state(&self) -> bool {
        !matches!(self, Self::Client)
    }

    /// The pools the match's scripts draw from; none for a match that runs no scripts.
    pub const fn pools(&self) -> Option<ScriptPools> {
        match *self {
            Self::Server(pools) => Some(pools),
            Self::Rules | Self::Client => None,
        }
    }
}
