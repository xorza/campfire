use bevy_replicon::prelude::RepliconTick;
use campfire_net_core::NetTick;

/// A completed Replicon checkpoint resolved into network time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletedCheckpoint {
    /// Replicon's protocol tick for this completed checkpoint.
    pub replicon_tick: RepliconTick,
    /// Authoritative network tick carried by the checkpoint.
    pub tick: NetTick,
}
