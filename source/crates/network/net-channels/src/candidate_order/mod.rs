use tracing::debug;

use crate::send_candidate::SendCandidate;

/// Reusable scheduler scratch for ordering channel-held send candidates.
///
/// This type never holds channel queues and does not decide whether a packet was sent: the
/// packet session's budget stops a flush, and the order decides which messages wait.
#[derive(Debug, Default)]
pub(crate) struct CandidateOrder {
    /// Reused scratch naming the channel-held pending messages of the flush.
    candidates: Vec<SendCandidate>,
}

impl CandidateOrder {
    pub(crate) const fn candidates_mut(&mut self) -> &mut Vec<SendCandidate> {
        &mut self.candidates
    }

    pub(crate) fn candidates(&self) -> &[SendCandidate] {
        &self.candidates
    }

    pub(crate) fn clear(&mut self) {
        self.candidates.clear();
    }

    /// Apply priority and packet-packing order without taking holding of channel queues.
    ///
    /// Fragments precede singles within each priority group so the packet builder can fill the
    /// final fragment packets with the smallest remaining singles.
    pub(crate) fn prioritize(&mut self) {
        // Every candidate has deterministic tie-breakers, so the in-place unstable sort has
        // stable observable output without allocating temporary sort storage.
        self.candidates.sort_unstable_by(|a, b| {
            b.rank
                .priority
                .total_cmp(&a.rank.priority)
                .then_with(|| Self::packing_order(a, b))
        });
        debug!(
            candidates = self.candidates.len(),
            "priority ordering complete"
        );
    }

    /// Orders candidates which have the same effective priority for efficient, deterministic
    /// packet packing.
    ///
    /// Fragmented messages come first so the packet builder can append smaller single messages to
    /// the final, partially filled fragment packet. Singles are then ordered from smallest to
    /// largest. `send_order` and the channel id keep the order deterministic without comparing
    /// wrapping message ids: a channel gives each message of one shape its own send order.
    fn packing_order(a: &SendCandidate, b: &SendCandidate) -> core::cmp::Ordering {
        match (a.is_single(), b.is_single()) {
            (false, true) => core::cmp::Ordering::Less,
            (true, false) => core::cmp::Ordering::Greater,
            (true, true) => a.single_len.cmp(&b.single_len),
            (false, false) => core::cmp::Ordering::Equal,
        }
        .then_with(|| a.rank.send_order.cmp(&b.rank.send_order))
        .then_with(|| a.channel_id.cmp(&b.channel_id))
    }
}

#[cfg(test)]
mod tests;
