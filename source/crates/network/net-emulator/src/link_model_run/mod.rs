//! The state a [`LinkModel`] carries from packet to packet on one direction.

use campfire_net_core::Instant;
use core::num::NonZeroU128;
use core::time::Duration;

use crate::link_model::LinkModel;
use crate::loss_state::LossState;
use campfire_net_common::SplitMix64;

/// When the model delivers one packet, and when its duplicate arrives, if it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) at: Instant,
    pub(crate) copy_at: Option<Instant>,
}

/// One direction's run of a [`LinkModel`]: its Gilbert–Elliott state, its generator, and the
/// moment its queue has sent everything in it.
#[derive(Debug, Clone)]
pub(crate) struct LinkModelRun {
    loss_state: LossState,
    rng: SplitMix64,
    busy_until: Instant,
}

impl LinkModelRun {
    pub(crate) const fn new(seed: u64) -> Self {
        Self {
            loss_state: LossState::Good,
            rng: SplitMix64::new(seed),
            busy_until: Instant::ZERO,
        }
    }

    /// Decides the fate of a packet of `bytes` sent at `now`, or `None` when it is lost.
    ///
    /// The draws come in a fixed order: the loss state's transition and the loss, always; then a
    /// reorder when `reorder > 0`, the jitter when `jitter > 0` and the packet is not reordered, a
    /// duplicate when `duplicate > 0`, and the duplicate's jitter. A reorder, a jitter or a
    /// duplicate left off draws nothing.
    ///
    /// A duplicate is a second packet on the queue, as netem's is: it pays its bytes against the
    /// rate limit after the original, and is not made when the queue cannot hold it.
    pub(crate) fn admit(
        &mut self,
        model: &LinkModel,
        bytes: usize,
        now: Instant,
    ) -> Option<Delivery> {
        let (loss, transition) = match self.loss_state {
            LossState::Good => (model.good_loss, model.good_to_bad),
            LossState::Bad => (model.bad_loss, model.bad_to_good),
        };
        if self.rng.next_f32() < transition {
            self.loss_state = match self.loss_state {
                LossState::Good => LossState::Bad,
                LossState::Bad => LossState::Good,
            };
        }
        if self.rng.next_f32() < loss {
            return None;
        }
        let departure = self.depart(model, bytes, now)?;
        let reordered = model.reorder > 0.0 && self.rng.next_f32() < model.reorder;
        let at = if reordered {
            departure
        } else {
            departure + self.delay(model)
        };
        let duplicated = model.duplicate > 0.0 && self.rng.next_f32() < model.duplicate;
        let copy_at = if duplicated {
            self.depart(model, bytes, now)
                .map(|departure| departure + self.delay(model))
        } else {
            None
        };
        Some(Delivery { at, copy_at })
    }

    /// Queues `bytes` sent at `now` behind the rate limit, and returns when they have left, or
    /// `None` when the queue cannot hold them.
    ///
    /// The queue drains in order: a packet never leaves before the ones queued ahead of it, even
    /// when a phase change lifts the rate limit.
    fn depart(&mut self, model: &LinkModel, bytes: usize, now: Instant) -> Option<Instant> {
        let start = now.max(self.busy_until);
        let Some(rate) = model.rate else {
            return Some(start);
        };
        let bytes_per_second = NonZeroU128::from(rate.bytes_per_second).get();
        let queued = start.saturating_duration_since(now).as_nanos() * bytes_per_second / NANOS;
        let bytes_wide = u128::try_from(bytes).expect("a packet size fits in u128");
        if queued + bytes_wide > u128::from(rate.queue_bytes) {
            return None;
        }
        let sending = u64::try_from(bytes_wide * NANOS / bytes_per_second)
            .expect("a packet sends in less than 584 years");
        self.busy_until = start + Duration::from_nanos(sending);
        Some(self.busy_until)
    }

    /// The latency moved by a uniform draw in `[-jitter, jitter)`, at least zero.
    fn delay(&mut self, model: &LinkModel) -> Duration {
        if model.jitter.is_zero() {
            return model.latency;
        }
        let jitter = u64::try_from(model.jitter.as_nanos()).expect("a jitter below 584 years");
        let offset = i128::from(self.rng.below(2 * jitter)) - i128::from(jitter);
        let delay = i128::try_from(model.latency.as_nanos()).expect("a latency fits") + offset;
        Duration::from_nanos(u64::try_from(delay.max(0)).expect("a delay below 584 years"))
    }
}

const NANOS: u128 = 1_000_000_000;

#[cfg(feature = "internals")]
pub(crate) mod bench;
#[cfg(test)]
mod tests;
