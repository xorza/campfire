use core::time::Duration;
use tracing::trace;

use crate::link_estimate::LinkEstimate;

/// The fraction bits of the estimates, in nanoseconds: an update rounds at 2^-32 ns, and only the
/// estimate it gives rounds to the nanosecond.
const FRACTION_BITS: u32 = 32;

/// The reciprocal of the smoothed RTT's gain, 1/12: RFC 6298's estimate, whose own alpha is 1/8,
/// with a gain tuned for one sample each 100 ms (11-net-stack/time.md, Two estimates of one link
/// keep two promises).
const RTT_GAIN_RECIPROCAL: i128 = 12;
/// The reciprocal of the RTT's mean absolute deviation's gain, 1/6, where RFC 6298's beta is 1/4,
/// tuned as the RTT's gain is.
const DEVIATION_GAIN_RECIPROCAL: i128 = 6;

/// A sample above the smoothed RTT plus this many mean absolute deviations is an outlier, clamped
/// to that bound.
const OUTLIER_DEVIATIONS: i128 = 3;
/// A sample above this multiple of the smoothed RTT is clamped too, which bounds a sample when
/// the deviation is near zero.
const MAX_RTT_RELATIVE_INCREASE_FACTOR: i128 = 3;
/// A sample above the smoothed RTT plus this time is clamped too, which bounds a sample when the
/// smoothed RTT is near zero.
const MAX_RTT_ABSOLUTE_INCREASE: Duration = Duration::from_millis(500);
/// The clamp always lets a sample rise to 6/5 of the smoothed RTT, which a zero deviation would
/// otherwise forbid.
const MIN_RTT_CLAMP_NUMERATOR: i128 = 6;
const MIN_RTT_CLAMP_DENOMINATOR: i128 = 5;

/// Estimates RTT and jitter with exponentially weighted moving averages, in fixed point, each
/// updated on its difference from the sample.
#[derive(Debug, Default)]
pub(crate) struct RttEstimatorEwma {
    /// The averages, from the first sample on.
    smoothed: Option<Smoothed>,
    /// The samples taken since the last reset.
    samples: u32,
}

/// The averages of the samples, in 2^-32 ns.
#[derive(Debug, Clone, Copy)]
struct Smoothed {
    rtt: i128,
    /// The smoothed absolute deviation of the samples from the smoothed RTT: the basis for
    /// jitter.
    deviation: i128,
}

fn fine(duration: Duration) -> i128 {
    i128::try_from(duration.as_nanos()).unwrap() << FRACTION_BITS
}

/// The nearest nanosecond, a half up.
fn coarse(fine: i128) -> Duration {
    let nanos = (fine + (1 << (FRACTION_BITS - 1))) >> FRACTION_BITS;
    Duration::from_nanos(u64::try_from(nanos).expect("an estimate is not negative"))
}

impl RttEstimatorEwma {
    /// The estimate once a sample came: the smoothed RTT, and as jitter half the RTT's mean
    /// absolute deviation, since jitter is taken as symmetrical between the send and receive
    /// paths.
    pub(crate) fn estimate(&self) -> Option<LinkEstimate> {
        let Smoothed { rtt, deviation } = self.smoothed?;
        Some(LinkEstimate::new(coarse(rtt), coarse(deviation / 2)))
    }

    /// The samples taken since the last reset.
    pub(crate) const fn samples(&self) -> u32 {
        self.samples
    }

    /// Updates the RTT and jitter estimates with a new RTT sample.
    pub(crate) fn update_with_new_sample(&mut self, new_rtt_sample: Duration) {
        let mut sample = fine(new_rtt_sample);
        self.samples = self.samples.saturating_add(1);

        let Some(Smoothed {
            rtt: srtt,
            deviation,
        }) = self.smoothed
        else {
            // The first sample seeds the deviation with half its RTT, as RFC 6298 §2.2 does.
            self.smoothed = Some(Smoothed {
                rtt: sample,
                deviation: sample / 2,
            });
            trace!(estimate = ?self.estimate(), "RTT estimate seeded");
            return;
        };

        // The estimates need a few samples before a sample can be judged an outlier.
        if self.samples > 2 {
            let bound = (srtt + OUTLIER_DEVIATIONS * deviation)
                .min(srtt * MAX_RTT_RELATIVE_INCREASE_FACTOR)
                .min(srtt + fine(MAX_RTT_ABSOLUTE_INCREASE))
                .max(srtt * MIN_RTT_CLAMP_NUMERATOR / MIN_RTT_CLAMP_DENOMINATOR);
            if sample > bound {
                trace!(
                    sample = ?new_rtt_sample,
                    clamped_to = ?coarse(bound),
                    srtt = ?coarse(srtt),
                    deviation = ?coarse(deviation),
                    "RTT sample clamped as outlier."
                );
                sample = bound;
            }
        }

        let error = sample - srtt;
        self.smoothed = Some(Smoothed {
            rtt: srtt + error / RTT_GAIN_RECIPROCAL,
            deviation: deviation + (error.abs() - deviation) / DEVIATION_GAIN_RECIPROCAL,
        });
        trace!(
            estimate = ?self.estimate(),
            sample = ?coarse(sample),
            "RTT estimate updated"
        );
    }

    /// Resets the estimator to its initial state (no samples).
    pub(crate) const fn reset(&mut self) {
        self.smoothed = None;
        self.samples = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn a_new_estimator_holds_no_estimate() {
        assert_eq!(RttEstimatorEwma::default().estimate(), None);
    }

    /// The first sample is the RTT, its half the deviation, and a quarter the jitter. Two more
    /// equal samples keep the RTT and decay the deviation by 5/6 each: 50 ms · 5/6 / 2 =
    /// 20.833_333 ms, then 50 ms · 25/36 / 2 = 17.361_111 ms. A reset drops the estimate.
    #[test]
    fn stable_samples_keep_the_rtt_and_decay_the_jitter() {
        let mut estimator = RttEstimatorEwma::default();
        let mut jitters = vec![];
        for _ in 0..3 {
            estimator.update_with_new_sample(ms(100));
            let estimate = estimator.estimate().unwrap();
            assert_eq!(estimate.rtt(), ms(100));
            jitters.push(estimate.jitter());
        }
        assert_eq!(
            jitters,
            [
                ms(25),
                Duration::from_nanos(20_833_333),
                Duration::from_nanos(17_361_111)
            ]
        );
        estimator.reset();
        assert_eq!(estimator.estimate(), None);
    }

    /// After three 100 ms samples the deviation is 50 ms · 25/36 = 34.722_222 ms, so a 1 s sample
    /// clamps to 100 ms + 3 deviations = 204.166_667 ms, under the 300 ms and 600 ms caps and
    /// over the 120 ms floor. The smoothed RTT moves 1/12 of the way to it: 108.680_556 ms, where
    /// the raw sample would give 175 ms.
    #[test]
    fn an_outlier_clamps_to_three_deviations() {
        let mut estimator = RttEstimatorEwma::default();
        for _ in 0..3 {
            estimator.update_with_new_sample(ms(100));
        }
        estimator.update_with_new_sample(Duration::from_secs(1));
        assert_eq!(
            estimator.estimate().unwrap().rtt(),
            Duration::from_nanos(108_680_556)
        );
    }

    /// With no deviation, the floor of 6/5 of the smoothed RTT is the bound: after the RTT and
    /// the deviation settle to 100 ms and 0, a 200 ms sample clamps to 120 ms, and the RTT moves
    /// to 100 + 20/12 = 101.666_667 ms.
    #[test]
    fn a_zero_deviation_clamps_to_the_floor() {
        let mut estimator = RttEstimatorEwma {
            smoothed: Some(Smoothed {
                rtt: fine(ms(100)),
                deviation: 0,
            }),
            samples: 3,
        };
        estimator.update_with_new_sample(ms(200));
        assert_eq!(
            estimator.estimate().unwrap().rtt(),
            Duration::from_nanos(101_666_667)
        );
    }
}
