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

impl ConfirmedTick {
    /// Whether it is newer than `other`: of a later network tick, or of the same one and a newer
    /// Replicon tick, as several Replicon ticks may map to one network tick.
    pub(crate) fn is_newer_than(self, other: Self) -> bool {
        self.tick > other.tick
            || (self.tick == other.tick && self.replicon_tick.is_newer(other.replicon_tick))
    }
}
