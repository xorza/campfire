//! How one direction of a link carries packets.

use campfire_math::{Num, Rounding};
use core::time::Duration;

use crate::rate_limit::RateLimit;

/// How one direction of a link carries packets, in the terms of `tc-netem(8)`: a latency with
/// jitter, Gilbert–Elliott loss, duplication, reordering, and a rate limit with a finite queue
/// (11-net-stack/testing.md, One `LinkModel` for emulated and real links, in netem's terms). The
/// MTU and the seed are the link's, in a `DuplexModel` or a `LinkConditioning`. Its probabilities
/// are `Num`s, which a draw compares against 24 bits of a word, as `Rng::chance` does
/// (11-net-stack/time.md, One number type).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkModel {
    /// One-way delay of every packet. For a symmetric link this is half the round-trip time.
    pub latency: Duration,

    /// Each packet's delay moves by a uniform draw in `[-jitter, jitter)` around
    /// [`latency`](Self::latency), to the nanosecond; a delay below zero delivers at once. Two
    /// packets whose delays cross arrive in the other order, as in netem.
    pub jitter: Duration,

    /// Probability that a packet is lost in the good state (`0..=1`).
    pub good_loss: Num,

    /// Probability that a packet is lost in the bad state (`0..=1`).
    pub bad_loss: Num,

    /// Probability of going from the good state to the bad after each packet (`0..=1`).
    pub good_to_bad: Num,

    /// Probability of going from the bad state to the good after each packet (`0..=1`).
    pub bad_to_good: Num,

    /// Probability that a delivered packet arrives twice, at the same moment (`0..=1`).
    pub duplicate: Num,

    /// Probability that a packet skips [`latency`](Self::latency) and its jitter and arrives as
    /// soon as the rate limit lets it, ahead of the packets sent before it (`0..=1`), as
    /// netem's `reorder` does.
    pub reorder: Num,

    /// The link's speed and the queue in front of it, or `None` for no limit.
    pub rate: Option<RateLimit>,
}

impl Default for LinkModel {
    fn default() -> Self {
        Self::perfect()
    }
}

impl LinkModel {
    /// A link that delivers every packet at once.
    pub const fn perfect() -> Self {
        Self {
            latency: Duration::ZERO,
            jitter: Duration::ZERO,
            good_loss: Num::ZERO,
            bad_loss: Num::ZERO,
            good_to_bad: Num::ZERO,
            bad_to_good: Num::ZERO,
            duplicate: Num::ZERO,
            reorder: Num::ZERO,
            rate: None,
        }
    }

    /// Replaces [`latency`](Self::latency).
    #[must_use]
    pub const fn with_latency(mut self, latency: Duration) -> Self {
        self.latency = latency;
        self
    }

    /// Replaces [`jitter`](Self::jitter).
    #[must_use]
    pub const fn with_jitter(mut self, jitter: Duration) -> Self {
        self.jitter = jitter;
        self
    }

    /// Replaces [`duplicate`](Self::duplicate).
    #[must_use]
    pub fn with_duplicate(mut self, duplicate: Num) -> Self {
        debug_assert!(
            is_probability(duplicate),
            "Duplicate probability must be in 0..=1, got {duplicate}"
        );
        self.duplicate = duplicate;
        self
    }

    /// Replaces [`reorder`](Self::reorder).
    #[must_use]
    pub fn with_reorder(mut self, reorder: Num) -> Self {
        debug_assert!(
            is_probability(reorder),
            "Reorder probability must be in 0..=1, got {reorder}"
        );
        self.reorder = reorder;
        self
    }

    /// Replaces [`rate`](Self::rate).
    #[must_use]
    pub const fn with_rate(mut self, rate: RateLimit) -> Self {
        self.rate = Some(rate);
        self
    }

    /// Replaces the packet-loss probability model with a fixed loss
    /// probability. Every packet is lost independently with the probability
    /// `loss_probability`.
    ///
    /// Pick this when only the overall loss rate matters, and a bursty form
    /// when consecutive-loss behavior does.
    #[must_use]
    pub fn with_fixed_loss(mut self, loss_probability: Num) -> Self {
        debug_assert!(
            is_probability(loss_probability),
            "Loss probability must be in 0..=1, got {loss_probability}"
        );
        self.good_loss = loss_probability;
        self.bad_loss = loss_probability;
        self.good_to_bad = Num::ZERO;
        self.bad_to_good = Num::ZERO;
        self
    }

    /// Replaces the packet-loss probability model with the Simple Gilbert model
    /// (Hasslinger & Hohlfeld, MMB 2008, Table 2). Packet loss occurs in
    /// "bursts", periods of complete packet loss. No packets are lost
    /// during non-burst periods.
    ///
    /// Pick this when all you know about the link is how much it loses and
    /// how long the outages run. This is the usual case when hand-authoring a
    /// preset.
    ///
    /// `mean_loss` is the fraction of total packets lost and must be in `[0,
    /// 1)`. A loss rate of 1 means every packet is lost, i.e. an infinitely
    /// long burst, which this model cannot represent. Use
    /// [`with_fixed_loss`](Self::with_fixed_loss) for a
    /// fully-lossy link.
    ///
    /// `mean_bad_len` is the average number of consecutive packets spent in
    /// bad per visit, and must be `>= 1`.
    /// Because the bad state here loses every packet, a bad-state visit *is*
    /// an observed burst of consecutive losses.
    ///
    /// Equivalent to [`with_gilbert_loss`](Self::with_gilbert_loss) with
    /// `bad_loss = 1`.
    #[must_use]
    pub fn with_simple_gilbert_loss(self, mean_loss: Num, mean_bad_len: Num) -> Self {
        self.with_gilbert_elliott_loss(mean_loss, mean_bad_len, Num::ONE, Num::ZERO)
    }

    /// Replaces the packet-loss probability model with a Gilbert model
    /// (Gilbert, BSTJ 39(5), 1960). It is similar to
    /// [`with_simple_gilbert_loss`](Self::with_simple_gilbert_loss) but
    /// bursts are now "leaky" where only a fraction of a burst's packets are
    /// lost instead of all of them.
    ///
    /// Pick this when bursts should thin traffic rather than sever it.
    ///
    /// `mean_loss` is the fraction of total packets lost and must be in
    /// `[0, bad_loss)`.
    ///
    /// `mean_bad_len` is the average number of consecutive packets spent in
    /// bad per visit (`>= 1`). Packets are
    /// not always lost in the bad state now, so an *observed* run of
    /// consecutive losses may be shorter than the bad-state visit that
    /// contains it.
    ///
    /// `bad_loss` is the probability that a packet is lost while the chain
    /// is in bad.
    ///
    /// Equivalent to
    /// [`with_gilbert_elliott_loss`](Self::with_gilbert_elliott_loss) with
    /// `min_loss = 0`.
    #[must_use]
    pub fn with_gilbert_loss(self, mean_loss: Num, mean_bad_len: Num, bad_loss: Num) -> Self {
        self.with_gilbert_elliott_loss(mean_loss, mean_bad_len, bad_loss, Num::ZERO)
    }

    /// Replaces the packet-loss probability model with a full Gilbert–Elliott
    /// model (Elliott, BSTJ 42(5), 1963). It is similar to
    /// [`with_gilbert_loss`](Self::with_gilbert_loss) but now there is a
    /// minimum packet-loss probability, `min_loss`, even while the chain is
    /// in good.
    ///
    /// Pick this only when the minimum packet loss probability is known
    /// independently of the bursts. A capture of real network packet loss
    /// over any realistic length rarely distinguishes a floor from frequent
    /// short bursts.
    ///
    /// `mean_loss` is the fraction of total packets lost and must be in
    /// `[min_loss, bad_loss)`. `mean_loss` is a mixture of the
    /// two state densities, so it can't be outside them.
    ///
    /// `mean_bad_len` is the average number of consecutive packets spent in
    /// bad per visit (`>= 1`).
    ///
    /// `bad_loss` is the probability that a packet is lost while the chain
    /// is in bad.
    #[must_use]
    pub fn with_gilbert_elliott_loss(
        mut self,
        mean_loss: Num,
        mean_bad_len: Num,
        bad_loss: Num,
        min_loss: Num,
    ) -> Self {
        debug_assert!(
            is_probability(min_loss) && is_probability(bad_loss),
            "Loss probabilities must be in 0..=1, got min_loss {min_loss}, bad_loss {bad_loss}"
        );
        debug_assert!(
            min_loss <= mean_loss && mean_loss < bad_loss,
            "Mean loss must be in range [min_loss, bad_loss) = [{min_loss}, {bad_loss}), got {mean_loss}"
        );
        debug_assert!(
            mean_bad_len >= Num::ONE,
            "Mean bad-state visit length must be >= 1 (a visit is at least one packet), got {mean_bad_len}"
        );

        // A bad-state visit is a run of consecutive packets in the bad state, and that run
        // length is geometrically distributed with mean `1 / bad_to_good` (Hasslinger &
        // Hohlfeld, MMB 2008, Sec. 3: their `r = 1/ABEL`, where r is bad_to_good and ABEL is the
        // average burst length).
        let bad_to_good = Num::ONE
            .checked_div(mean_bad_len, Rounding::NearestEven)
            .expect("a visit of at least one packet");

        // Total loss rate is each state's loss density weighted by its stationary probability:
        // `mean_loss = (1 - stationary_bad) * min_loss + stationary_bad * bad_loss` (Hasslinger &
        // Hohlfeld, MMB 2008, Eq. 2), so `stationary_bad = (mean_loss - min_loss) / (bad_loss -
        // min_loss)`; with `stationary_bad = good_to_bad / (good_to_bad + bad_to_good)`, the
        // transition is `bad_to_good · (mean_loss - min_loss) / (bad_loss - mean_loss)`, from its
        // exact product rounded once.
        let good_to_bad = (mean_loss - min_loss)
            .checked_mul_div(bad_to_good, bad_loss - mean_loss, Rounding::NearestEven)
            .expect("a mean loss below the bad state's");

        self.good_loss = min_loss;
        self.bad_loss = bad_loss;
        self.good_to_bad = good_to_bad;
        self.bad_to_good = bad_to_good;
        self
    }

    /// The fraction of total packets this configuration drops: each state's loss probability
    /// weighted by how often the chain is in it, `good_loss + stationary_bad · (bad_loss -
    /// good_loss)` with `stationary_bad = good_to_bad / (good_to_bad + bad_to_good)`, from the
    /// exact product rounded once (Gilbert-Elliott: Elliott, BSTJ 42(5), 1963; Hasslinger &
    /// Hohlfeld, MMB 2008, Eq. 2). With no transitions the chain never leaves the good state.
    pub fn mean_loss(&self) -> Num {
        let transition = self.good_to_bad + self.bad_to_good;
        if transition == Num::ZERO {
            return self.good_loss;
        }
        self.good_loss
            + self
                .good_to_bad
                .checked_mul_div(
                    self.bad_loss - self.good_loss,
                    transition,
                    Rounding::NearestEven,
                )
                .expect("a weight of at most 1")
    }

    /// An approximate one-way half of this model.
    ///
    /// This divides latency, jitter, and loss probabilities by two.
    #[must_use]
    pub fn half(self) -> Self {
        Self {
            latency: self.latency / 2,
            jitter: self.jitter / 2,
            good_loss: half(self.good_loss),
            bad_loss: half(self.bad_loss),
            // Don't halve the state-transition probabilities. It would
            // only stretch the timescale of the bursts of packet loss.
            ..self
        }
    }

    /// A preset for a low-latency, low-loss connection.
    pub fn good_condition() -> Self {
        Self::default()
            .with_latency(Duration::from_millis(40))
            .with_jitter(Duration::from_millis(6))
            .with_fixed_loss(per_mille(2))
    }

    /// A preset for a typical moderate-latency connection.
    pub fn average_condition() -> Self {
        Self::default()
            .with_latency(Duration::from_millis(100))
            .with_jitter(Duration::from_millis(15))
            .with_fixed_loss(per_mille(20))
    }

    /// A preset for a high-latency, lossy connection.
    pub fn poor_condition() -> Self {
        Self::default()
            .with_latency(Duration::from_millis(200))
            .with_jitter(Duration::from_millis(30))
            .with_fixed_loss(per_mille(100))
    }
}

/// Whether `value` is a probability, in `[0, 1]`.
fn is_probability(value: Num) -> bool {
    (Num::ZERO..=Num::ONE).contains(&value)
}

/// Half of `value`, to the nearest 2^-24, a tie to the even step.
fn half(value: Num) -> Num {
    value
        .checked_div_int(2, Rounding::NearestEven)
        .expect("a half fits")
}

/// `thousandths` / 1000, to the nearest 2^-24.
pub(crate) fn per_mille(thousandths: i64) -> Num {
    Num::int(thousandths)
        .checked_div_int(1000, Rounding::NearestEven)
        .expect("a share of a thousand fits")
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;

    /// A test's probability of `thousandths` / 1000, for a crate that names no `Num`.
    pub fn per_mille(thousandths: i64) -> Num {
        super::per_mille(thousandths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(text: &str) -> Num {
        text.parse().unwrap()
    }

    #[test]
    fn a_fixed_loss_is_the_same_in_each_state() {
        let setup = LinkModel::default().with_fixed_loss(num("0.3"));
        // Both `LossState` states lose packets at the same rate, so the mean loss
        // is exactly the rate regardless of which state the chain sits in.
        assert_eq!(setup.mean_loss(), num("0.3"));
        assert_eq!(setup.good_loss, setup.bad_loss);
    }

    #[test]
    fn the_default_model_loses_nothing() {
        assert_eq!(LinkModel::default().mean_loss(), Num::ZERO);
    }

    /// Each constructor's transitions, in steps of 2^-24, and the mean loss they give back. At a
    /// mean of 0.02 over visits of 4 packets that lose all: `bad_to_good` is 1/4, 4,194,304
    /// steps, and `good_to_bad` is 1/4 · 0.02 / 0.98, 85,597.8… steps from the literal's
    /// 335,544, to the nearest 85,598, whose mean loss is 335,544 again. A visit of 3 is
    /// 5,592,405.3…, to 5,592,405, and of 6 is 2,796,202.6…, to 2,796,203; the mean loss of each
    /// row is its literal's step, or one below where its two roundings add up.
    #[test]
    fn each_loss_model_hits_its_mean_loss() {
        let rows = [
            // mean, visit, bad loss, min loss: good_to_bad, bad_to_good, mean loss
            (("0.002", "3", "1", "0"), (11_207, 5_592_405, 33_554)),
            (("0.02", "4", "1", "0"), (85_598, 4_194_304, 335_544)),
            (("0.10", "6", "1", "0"), (310_689, 2_796_203, 1_677_721)),
            (("0.5", "10", "1", "0"), (1_677_722, 1_677_722, 8_388_608)),
            (("0.002", "3", "0.5", "0"), (22_459, 5_592_405, 33_554)),
            (("0.02", "4", "0.8", "0"), (107_546, 4_194_304, 335_544)),
            (("0.10", "6", "0.3", "0"), (1_398_102, 2_796_203, 1_677_722)),
            (("0.01", "3", "0.5", "0.001"), (102_718, 5_592_405, 167_772)),
            (("0.05", "8", "0.8", "0.01"), (111_848, 2_097_152, 838_860)),
            (
                ("0.10", "6", "0.3", "0.02"),
                (1_118_482, 2_796_203, 1_677_722),
            ),
        ];
        for ((mean, visit, bad, min), (good_to_bad, bad_to_good, mean_loss)) in rows {
            let setup = LinkModel::default().with_gilbert_elliott_loss(
                num(mean),
                num(visit),
                num(bad),
                num(min),
            );
            assert_eq!(
                (setup.good_to_bad, setup.bad_to_good, setup.mean_loss()),
                (
                    Num::from_bits(good_to_bad),
                    Num::from_bits(bad_to_good),
                    Num::from_bits(mean_loss)
                ),
                "{mean} over visits of {visit}, {bad} in bad and {min} in good"
            );
            assert_eq!((setup.good_loss, setup.bad_loss), (num(min), num(bad)));
        }
    }

    #[test]
    fn each_loss_constructor_is_the_one_above_it_with_a_parameter_fixed() {
        // Each constructor with its extra knob pinned must produce exactly the
        // rung below it.
        let (mean_loss, mean_bad_len) = (num("0.02"), Num::int(4));
        let base = LinkModel::default;
        let simple = base().with_simple_gilbert_loss(mean_loss, mean_bad_len);
        assert_eq!(
            simple,
            base().with_gilbert_loss(mean_loss, mean_bad_len, Num::ONE)
        );
        assert_eq!(
            base().with_gilbert_loss(mean_loss, mean_bad_len, Num::HALF),
            base().with_gilbert_elliott_loss(mean_loss, mean_bad_len, Num::HALF, Num::ZERO)
        );
    }

    /// The presets' losses are their thousandths to the nearest step: 0.002 is 33,554.4… steps,
    /// to 33,554. Half of 33,554 is 16,777, and half of 33,555 a tie, to the even 16,778.
    #[test]
    fn a_share_and_a_half_round_to_the_nearest_step() {
        assert_eq!(per_mille(2), Num::from_bits(33_554));
        assert_eq!(LinkModel::good_condition().good_loss, per_mille(2));
        assert_eq!(half(Num::from_bits(33_554)), Num::from_bits(16_777));
        assert_eq!(half(Num::from_bits(33_555)), Num::from_bits(16_778));
        assert_eq!(half(Num::from_bits(33_557)), Num::from_bits(16_778));
    }
}
