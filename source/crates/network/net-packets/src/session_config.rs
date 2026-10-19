use core::time::Duration;

use crate::bandwidth_limit::BandwidthLimit;

/// How a session proves it lives, how long a silent one lasts, and how fast it sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionConfig {
    /// A session that sent nothing for this long sends a `Ping`.
    pub keep_alive: Duration,
    /// A session that heard nothing authentic for this long ends with `TimedOut`.
    pub idle_timeout: Duration,
    /// The rate the session sends its payloads at, below its congestion window; `None` for the
    /// window alone.
    pub bandwidth: Option<BandwidthLimit>,
}

impl Default for SessionConfig {
    /// A keep-alive each second, a loss after 5 s, and no rate below the window.
    fn default() -> Self {
        Self {
            keep_alive: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(5),
            bandwidth: None,
        }
    }
}
