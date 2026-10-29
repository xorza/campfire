use core::time::Duration;

use bytes::Bytes;
use campfire_net_core::Instant;

use crate::direction_counts::DirectionCounts;
use crate::link_model::LinkModel;
use crate::link_model_run::{Arrival, LinkModelRun};
use crate::phase::Phase;
use crate::phase_mode::PhaseMode;
use crate::ready_buffer::ReadyBuffer;

/// One direction of an emulated link: its model, its timeline of phases, and the packets on it.
#[derive(Debug)]
pub(crate) struct EmulatedDirection {
    base: LinkModel,
    /// Sorted by start.
    phases: Vec<Phase>,
    run: LinkModelRun,
    /// Packets due together leave in the order they were sent.
    in_flight: ReadyBuffer<Instant, Bytes>,
    pub(crate) counts: DirectionCounts,
}

impl EmulatedDirection {
    pub(crate) const fn new(model: LinkModel, seed: u64) -> Self {
        Self {
            run: LinkModelRun::new(seed),
            base: model,
            phases: Vec::new(),
            in_flight: ReadyBuffer::new(),
            counts: DirectionCounts {
                sent: 0,
                sent_bytes: 0,
                dropped: 0,
                duplicated: 0,
                delivered: 0,
            },
        }
    }

    pub(crate) fn add_phase(&mut self, phase: Phase) {
        let at = self
            .phases
            .partition_point(|other| other.start <= phase.start);
        self.phases.insert(at, phase);
    }

    /// Puts `payload` on the wire at wire time `now`, under the phase that holds then; one past
    /// the link's `mtu` is dropped in any phase.
    pub(crate) fn send(&mut self, payload: Bytes, now: Duration, mtu: Option<usize>) {
        self.counts.sent += 1;
        self.counts.sent_bytes += u64::try_from(payload.len()).unwrap();
        let fits = mtu.is_none_or(|mtu| payload.len() <= mtu);
        let delivery = if fits {
            self.admit(payload.len(), now)
        } else {
            None
        };
        let Some(delivery) = delivery else {
            self.counts.dropped += 1;
            return;
        };
        if let Some(copy_at) = delivery.copy_at {
            self.counts.duplicated += 1;
            self.in_flight.push(delivery.at, payload.clone());
            self.in_flight.push(copy_at, payload);
        } else {
            self.in_flight.push(delivery.at, payload);
        }
    }

    /// When a packet of `bytes` sent at wire time `now` arrives, under the phase that holds then,
    /// or `None` when it is lost.
    fn admit(&mut self, bytes: usize, now: Duration) -> Option<Arrival> {
        let current = self.phases.partition_point(|phase| phase.start <= now);
        let (model, hold_until) = match current.checked_sub(1).map(|i| &self.phases[i]) {
            None => (&self.base, None),
            Some(phase) => {
                let next_start = self.phases.get(current).map(|next| next.start);
                match phase.mode {
                    PhaseMode::Pass => (&phase.model, None),
                    PhaseMode::Hold => (&phase.model, Some(next_start?)),
                }
            }
        };
        let delivery = self.run.admit(model, bytes, Instant::from_elapsed(now))?;
        let held =
            |at: Instant| hold_until.map_or(at, |until| at.max(Instant::from_elapsed(until)));
        Some(Arrival {
            at: held(delivery.at),
            copy_at: delivery.copy_at.map(held),
        })
    }

    /// The wire time the next packet in flight is due at.
    pub(crate) fn next_due(&self) -> Option<Duration> {
        self.in_flight.next_key().map(|at| at.elapsed())
    }

    /// Hands the packets due by wire time `now` to `deliver`, in the order they are due, at
    /// most `max` of them, and returns how many: the rest stay due.
    pub(crate) fn deliver_due(
        &mut self,
        now: Duration,
        max: usize,
        mut deliver: impl FnMut(Bytes),
    ) -> usize {
        let now = Instant::from_elapsed(now);
        let mut delivered = 0;
        while delivered < max
            && let Some(payload) = self.in_flight.pop_ready(&now)
        {
            self.counts.delivered += 1;
            delivered += 1;
            deliver(payload);
        }
        delivered
    }
}
