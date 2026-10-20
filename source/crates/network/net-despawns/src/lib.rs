//! The one despawn of the network stack's replication crates, which their `clippy.toml` denies
//! Bevy's own (13-net-stack/replication.md, The server alone ends a replicated entity's life on
//! the receiver).

#![expect(
    clippy::disallowed_methods,
    reason = "the one crate that despawns for the replication crates, whose clippy.toml denies it"
)]

mod despawn_replicated;

pub use crate::despawn_replicated::DespawnReplicated;
