use bevy_reflect::Reflect;
use core::time::Duration;

/// How a `LinkEstimator` takes the link's RTT samples.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct LinkEstimatorConfig {
    /// The time between two samples the estimate takes, whatever the session gives: the
    /// estimate's gains, tuned for this rate, then keep their time constants on every link.
    pub sample_interval: Duration,
    /// How many samples the estimate takes as they come before the timelines that read it start.
    pub handshake_samples: u8,
}

impl Default for LinkEstimatorConfig {
    fn default() -> Self {
        Self {
            sample_interval: Duration::from_millis(100),
            handshake_samples: 3,
        }
    }
}
