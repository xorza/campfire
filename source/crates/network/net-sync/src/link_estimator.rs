//! The latency estimate of a link, from the RTT samples its packet session takes from each ACK.

use bevy_ecs::prelude::*;
use campfire_net_connection::{Connected, HostClient};
use campfire_net_link::Link;
use campfire_net_timeline::Clock;
use core::time::Duration;
use tracing::trace;

use crate::link_estimate::LinkEstimate;
use crate::link_estimator_setup::LinkEstimatorSetup;
use crate::rtt_estimator_ewma::RttEstimatorEwma;

/// Filters the RTT samples of a link's packet session into the estimate that timeline sync reads.
///
/// The packet session keeps its own estimate for its timers; this one keeps a stable input delay,
/// so it takes one sample each [`LinkEstimatorSetup::sample_interval`] after the first
/// [`LinkEstimatorSetup::handshake_samples`], however often the packet session acknowledges
/// (13-net-stack/packet-session.md, The RTT comes from the ACKs).
#[derive(Debug, Component)]
pub struct LinkEstimator {
    setup: LinkEstimatorSetup,
    /// The time since the last sample the estimate took, less the intervals it covered.
    since_sample: Duration,
    rtt_estimator_ewma: RttEstimatorEwma,
}

impl Default for LinkEstimator {
    fn default() -> Self {
        Self::new(LinkEstimatorSetup::default())
    }
}

impl LinkEstimator {
    pub fn new(setup: LinkEstimatorSetup) -> Self {
        Self {
            setup,
            since_sample: Duration::ZERO,
            rtt_estimator_ewma: RttEstimatorEwma::default(),
        }
    }

    /// The estimate once it holds the samples [`LinkEstimatorSetup::handshake_samples`] asks for
    /// (13-net-stack/time.md, An unknown estimate is `None`).
    pub fn estimate(&self) -> Option<LinkEstimate> {
        if self.samples() < u32::from(self.setup.handshake_samples) {
            return None;
        }
        self.rtt_estimator_ewma.estimate()
    }

    /// The samples the estimate took.
    pub const fn samples(&self) -> u32 {
        self.rtt_estimator_ewma.samples()
    }

    /// Keeps the setup.
    pub(crate) const fn reset(&mut self) {
        self.since_sample = Duration::ZERO;
        self.rtt_estimator_ewma.reset();
    }

    /// Takes the frame's `samples` after a frame of `delta`: each one during the handshake, then
    /// one each interval. The part of an interval a frame overran counts toward the next, so the
    /// samples keep the interval's rate across frames; a gap of several intervals gives one
    /// sample, not a burst.
    pub(crate) fn take(&mut self, delta: Duration, samples: &[Duration]) {
        self.since_sample += delta;
        let interval = self.setup.sample_interval;
        for &sample in samples {
            let handshake = self.samples() < u32::from(self.setup.handshake_samples);
            if !handshake && self.since_sample < interval {
                continue;
            }
            if !handshake {
                let overrun = self.since_sample - interval;
                self.since_sample = if overrun < interval {
                    overrun
                } else if interval.is_zero() {
                    Duration::ZERO
                } else {
                    Duration::from_nanos(
                        u64::try_from(overrun.as_nanos() % interval.as_nanos()).unwrap(),
                    )
                };
            }
            self.rtt_estimator_ewma.update_with_new_sample(sample);
            trace!(
                target: "campfire_net_debug::sync",
                kind = "rtt_sample",
                ?sample,
                samples = self.samples(),
                estimate = ?self.rtt_estimator_ewma.estimate(),
                "took the link's RTT sample"
            );
        }
    }

    /// Takes each connected link's samples of the frame, which its packet session handed over.
    pub(crate) fn sample(
        clock: Res<'_, Clock>,
        mut links: Query<'_, '_, (&Link, &mut Self), (With<Connected>, Without<HostClient>)>,
    ) {
        let delta = clock.delta();
        for (link, mut estimator) in &mut links {
            estimator.take(delta, link.packet_session.rtt_samples());
        }
    }

    /// A new connection starts its estimate again.
    pub(crate) fn handle_connect(
        trigger: On<'_, '_, Add<Connected>>,
        mut query: Query<'_, '_, &mut Self>,
    ) {
        if let Ok(mut estimator) = query.get_mut(trigger.entity) {
            estimator.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// The first 3 samples are taken as they come, in a frame of 10 ms, and the fourth waits;
    /// then one each 100 ms. Frames of 30 ms with a sample each reach the interval at 90 ms,
    /// 10 + 3 · 30 = 100, and next at 210 ms, 4 · 30 = 120, whose overrun of 20 ms counts toward
    /// the interval after.
    #[test]
    fn the_handshake_takes_each_sample_then_one_each_interval() {
        let mut estimator = LinkEstimator::default();
        estimator.take(ms(10), &[ms(50), ms(50), ms(50), ms(50)]);
        assert_eq!(
            estimator.samples(),
            3,
            "the handshake's 3, and not the fourth"
        );
        let mut taken = vec![];
        for frame in 1..=7_u64 {
            let before = estimator.samples();
            estimator.take(ms(30), &[ms(50)]);
            if estimator.samples() > before {
                taken.push(frame * 30);
            }
        }
        assert_eq!(taken, [90, 210]);
    }

    /// The estimate waits for the handshake's samples: with the default 3, two samples of
    /// 100 ms give none, and the third gives the round trip, with a first deviation of half the
    /// first sample, decayed twice by 5/6, and a jitter of half that deviation.
    #[test]
    fn the_estimate_waits_for_the_handshake() {
        let mut estimator = LinkEstimator::default();
        estimator.take(ms(10), &[ms(100), ms(100)]);
        assert_eq!(estimator.estimate(), None);
        estimator.take(ms(10), &[ms(100)]);
        let estimate = estimator.estimate().unwrap();
        assert_eq!(estimate.rtt(), ms(100));
        // 50 ms · (5/6)^2 / 2 = 17.361_111 ms.
        assert_eq!(estimate.jitter(), Duration::from_nanos(17_361_111));
    }

    /// A gap of 1 s with no sample takes one sample at its end, not ten; an interval of zero
    /// takes every sample; a reset drops the estimate and its samples.
    #[test]
    fn a_gap_takes_one_sample_and_a_zero_interval_takes_all() {
        let mut estimator = LinkEstimator::default();
        estimator.take(ms(0), &[ms(50); 3]);
        estimator.take(Duration::from_secs(1), &[ms(50); 4]);
        assert_eq!(estimator.samples(), 4);

        let mut every = LinkEstimator::new(LinkEstimatorSetup {
            sample_interval: Duration::ZERO,
            ..LinkEstimatorSetup::default()
        });
        every.take(ms(0), &[ms(50); 7]);
        assert_eq!(every.samples(), 7);
        every.reset();
        assert_eq!((every.samples(), every.estimate()), (0, None));
    }
}
