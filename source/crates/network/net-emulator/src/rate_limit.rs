use core::num::NonZeroU32;

/// A link's speed and the queue in front of it (13-net-stack/testing.md, A link's queue drains in
/// order across phase changes).
///
/// A packet leaves once the link has sent every packet queued before it, which takes its size over
/// the speed; one that would make the queue hold more than `queue_bytes` is dropped at the queue's
/// tail. A slow link with a deep queue therefore delays packets more as the load grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimit {
    pub bytes_per_second: NonZeroU32,
    pub queue_bytes: u32,
}

impl RateLimit {
    pub const fn new(bytes_per_second: u32, queue_bytes: u32) -> Self {
        Self {
            bytes_per_second: NonZeroU32::new(bytes_per_second).expect("a rate is not zero"),
            queue_bytes,
        }
    }
}
