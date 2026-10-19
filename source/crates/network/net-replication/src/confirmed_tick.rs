use bevy_replicon::prelude::RepliconTick;
use campfire_net_core::NetTick;

/// A completed Replicon confirmed tick resolved into network time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfirmedTick {
    /// Replicon's protocol tick for this completed confirmed tick.
    pub replicon_tick: RepliconTick,
    /// Authoritative network tick carried by the confirmed tick.
    pub tick: NetTick,
}
