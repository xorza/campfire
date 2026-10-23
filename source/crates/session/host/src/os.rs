use bevy_ecs::resource::Resource;
use campfire_protocol::Entropy;

use crate::unix_clock::UnixClock;

/// What a setup takes from the operating system, or from a test in its place: the clock and the
/// random bytes. A client keeps it as a resource, for the random bytes of its retries' waits.
#[derive(Resource, Debug, Clone)]
pub struct Os {
    pub clock: UnixClock,
    pub entropy: Entropy,
}

impl Os {
    /// The operating system's clock and random bytes, which a binary's setups take.
    pub const SYSTEM: Self = Self {
        clock: UnixClock::System,
        entropy: Entropy::System,
    };
}
