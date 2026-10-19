use core::time::Duration;

use crate::bandwidth_limit::BandwidthLimit;

/// How a packet session proves it lives, how long a silent one lasts, and how fast it sends
/// (13-net-stack/packet-session.md, The keep-alive and the idle timeout are the packet session's)
/// (13-net-stack/testing.md, The packet session times a silent peer out, on every IO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketSessionSetup {
    /// A packet session that sent nothing for this long sends a `Ping`.
    pub keep_alive: Duration,
    /// A packet session that heard nothing authentic for this long ends with `TimedOut`.
    pub idle_timeout: Duration,
    /// The rate the packet session sends its payloads at, below its congestion window; `None` for
    /// the window alone.
    pub bandwidth: Option<BandwidthLimit>,
}

impl Default for PacketSessionSetup {
    /// A keep-alive each second, a loss after 5 s, and no rate below the window.
    fn default() -> Self {
        Self {
            keep_alive: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(5),
            bandwidth: None,
        }
    }
}
