//! Receive-side packet conditioning: a [`LinkModel`] applied to the datagrams an endpoint receives.
use campfire_net_core::Instant;

use crate::link_conditioning::LinkConditioning;
use crate::link_model::LinkModel;
use crate::link_model_run::LinkModelRun;
use crate::ready_buffer::ReadyBuffer;

/// Generic receive-side packet conditioner (11-net-stack/packet-session.md, The link conditioner
/// goes into the endpoint, before the packet sessions).
///
/// `LinkConditioner` delays, drops and duplicates payloads according to a [`LinkModel`], so a
/// developer plays over a real IO with an emulated network in front of it.
#[derive(Debug, Clone)]
pub struct LinkConditioner<P> {
    model: LinkModel,
    run: LinkModelRun,
    /// Payloads waiting for their emulated delivery instant.
    time_queue: ReadyBuffer<Instant, P>,
}

impl<P: Clone> LinkConditioner<P> {
    /// Creates an empty conditioner of `conditioning`, or of a model whose draws follow seed 0.
    pub fn new(conditioning: impl Into<LinkConditioning>) -> Self {
        let conditioning = conditioning.into();
        Self {
            run: LinkModelRun::new(conditioning.seed),
            model: conditioning.model,
            time_queue: ReadyBuffer::new(),
        }
    }

    /// Applies the model to `packet`, of `bytes` bytes, received at `instant`.
    ///
    /// Dropped packets are discarded immediately. Delivered packets are queued by their emulated
    /// delivery instant, twice when the model duplicates them.
    pub fn condition_packet(&mut self, packet: P, bytes: usize, instant: Instant) {
        let Some(delivery) = self.run.admit(&self.model, bytes, instant) else {
            return;
        };
        if let Some(copy_at) = delivery.copy_at {
            self.time_queue.push(delivery.at, packet.clone());
            self.time_queue.push(copy_at, packet);
        } else {
            self.time_queue.push(delivery.at, packet);
        }
    }

    /// The next packet whose delivery instant has elapsed.
    pub fn pop_packet(&mut self, instant: Instant) -> Option<P> {
        self.time_queue.pop_ready(&instant)
    }

    /// The instant the next held packet is due at.
    pub fn next_due(&self) -> Option<Instant> {
        self.time_queue.next_key().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_math::Num;
    use core::time::Duration;

    #[test]
    fn a_duplicated_packet_is_queued_twice() {
        let mut conditioner = LinkConditioner::new(
            LinkModel::perfect()
                .with_latency(Duration::from_millis(30))
                .with_duplicate(Num::ONE),
        );
        conditioner.condition_packet(7_u32, 100, Instant::ZERO);
        let due = Instant::from_elapsed(Duration::from_millis(30));
        assert_eq!(
            conditioner.pop_packet(due.saturating_sub(Duration::from_millis(1))),
            None,
            "nothing is due before the latency"
        );
        assert_eq!(conditioner.pop_packet(due), Some(7));
        assert_eq!(conditioner.pop_packet(due), Some(7));
        assert_eq!(conditioner.pop_packet(due), None);
    }

    /// A frame's packets share one receive instant, so a constant latency makes them due together.
    #[test]
    fn packets_due_at_one_instant_leave_in_their_arrival_order() {
        let mut conditioner =
            LinkConditioner::new(LinkModel::perfect().with_latency(Duration::from_millis(30)));
        for packet in 0..8_u32 {
            conditioner.condition_packet(packet, 100, Instant::ZERO);
        }
        let due = Instant::from_elapsed(Duration::from_millis(30));
        let delivered = core::iter::from_fn(|| conditioner.pop_packet(due)).collect::<Vec<_>>();
        assert_eq!(delivered, (0..8).collect::<Vec<_>>());
    }
}
