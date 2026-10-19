//! The round-trip estimate the packet session's timers read, as RFC 9002 §5 keeps it.

use core::time::Duration;

use crate::received_window::MAX_ACK_DELAY;

/// The RTT before the first sample (RFC 9002 §6.2.2, `kInitialRtt`).
pub(crate) const INITIAL_RTT: Duration = Duration::from_millis(333);

/// The smallest span a timer tells apart (RFC 9002 §6.1.2, `kGranularity`).
pub(crate) const GRANULARITY: Duration = Duration::from_millis(1);

/// RFC 9002's estimate of a link's RTT, for the packet session's timers (13-net-stack/time.md, An
/// RTT sample is exact).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RttEstimator {
    latest: Duration,
    smoothed: Duration,
    variance: Duration,
    /// The least sample, `None` before the first.
    min: Option<Duration>,
}

impl Default for RttEstimator {
    /// Before a sample: the smoothed RTT is `INITIAL_RTT` and its variance half of it.
    fn default() -> Self {
        Self {
            latest: INITIAL_RTT,
            smoothed: INITIAL_RTT,
            variance: INITIAL_RTT / 2,
            min: None,
        }
    }
}

impl RttEstimator {
    pub(crate) const fn smoothed(&self) -> Duration {
        self.smoothed
    }

    pub(crate) const fn variance(&self) -> Duration {
        self.variance
    }

    /// The larger of the smoothed and the latest RTT, which the loss delay scales.
    pub(crate) fn max_recent(&self) -> Duration {
        self.smoothed.max(self.latest)
    }

    pub(crate) const fn has_sample(&self) -> bool {
        self.min.is_some()
    }

    /// Takes the sample `latest`, whose acknowledgement waited `ack_delay` at the peer, and
    /// returns it less that whole delay, as a reader outside the packet session takes it. The first
    /// sample sets the estimate; each later one moves it by RFC 9002 §5.3's gains, 1/8 and 1/4,
    /// with the delay capped at `MAX_ACK_DELAY` taken off, unless that would put it below the
    /// least RTT.
    pub(crate) fn update(&mut self, latest: Duration, ack_delay: Duration) -> Duration {
        self.latest = latest;
        let sample = latest.saturating_sub(ack_delay);
        let Some(min) = self.min else {
            self.min = Some(latest);
            self.smoothed = latest;
            self.variance = latest / 2;
            return sample;
        };
        let min = min.min(latest);
        self.min = Some(min);
        let ack_delay = ack_delay.min(MAX_ACK_DELAY);
        let adjusted = if latest >= min + ack_delay {
            latest - ack_delay
        } else {
            latest
        };
        let deviation = self.smoothed.abs_diff(adjusted);
        self.variance = (self.variance * 3 + deviation) / 4;
        self.smoothed = (self.smoothed * 7 + adjusted) / 8;
        sample
    }

    /// The probe timeout before backoff: `smoothed + max(4·variance, GRANULARITY) +
    /// MAX_ACK_DELAY` (RFC 9002 §6.2.1).
    pub(crate) fn probe_timeout(&self) -> Duration {
        self.smoothed + (self.variance * 4).max(GRANULARITY) + MAX_ACK_DELAY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// Before a sample the probe timeout is 333 + 4 · 166.5 + 25 = 1,024 ms. The first sample of
    /// 100 ms sets the smoothed RTT to 100 and its variance to 50. A second of 120 ms with 8 ms of
    /// ACK delay is 112 ms adjusted: variance (3 · 50 + 12) / 4 = 40.5, smoothed (7 · 100 + 112)
    /// / 8 = 101.5.
    #[test]
    fn samples_move_the_estimate_by_the_rfcs_gains() {
        let mut rtt = RttEstimator::default();
        assert_eq!(rtt.probe_timeout(), ms(1_024));
        assert_eq!(rtt.update(ms(100), Duration::ZERO), ms(100));
        assert_eq!([rtt.smoothed(), rtt.variance()], [ms(100), ms(50)]);
        assert_eq!(rtt.update(ms(120), ms(8)), ms(112));
        assert_eq!(rtt.variance(), Duration::from_micros(40_500));
        assert_eq!(rtt.smoothed(), Duration::from_micros(101_500));
        assert_eq!(
            rtt.probe_timeout(),
            Duration::from_micros(101_500 + 162_000 + 25_000)
        );
    }

    /// The estimate takes the delay capped at 25 ms off, and none that would put a sample below
    /// the least: after a least of 100 ms, 110 ms with 30 ms of delay moves it as 110 ms, as
    /// 110 − 25 = 85 is below 100, to (7 · 100 + 110) / 8 = 101.25 ms; then 140 ms with 30 ms
    /// moves it as 140 − 25 = 115 ms, to (7 · 101.25 + 115) / 8 = 102.96875 ms. The sample a
    /// reader takes is less the whole delay: 80 ms, then 110 ms.
    #[test]
    fn the_estimate_caps_the_delay_and_the_sample_takes_it_whole() {
        let mut rtt = RttEstimator::default();
        rtt.update(ms(100), Duration::ZERO);
        assert_eq!(rtt.update(ms(110), ms(30)), ms(80));
        assert_eq!(rtt.smoothed(), Duration::from_micros(101_250));
        assert_eq!(rtt.update(ms(140), ms(30)), ms(110));
        assert_eq!(rtt.smoothed(), Duration::from_nanos(102_968_750));
    }
}
