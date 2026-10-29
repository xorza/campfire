use campfire_math::TickRate;

use crate::link_estimate::LinkEstimate;
use crate::remote_timeline::RemoteTimeline;

/// What one synchronization sample measures a controlled clock against.
#[derive(Debug, Clone, Copy)]
pub struct SyncSample<'a, C> {
    pub remote: &'a RemoteTimeline,
    pub setup: &'a C,
    /// The link's latency estimate.
    pub estimate: LinkEstimate,
    pub tick_rate: TickRate,
}
