/// The room left in one transport's receive budget while it reads a packet
/// (11-net-stack/transport.md, Each link's `Channels` has one receive budget).
///
/// The budget bounds the bytes a transport's receive side holds for messages it can not deliver
/// yet: reassembly buffers, and ordered messages waiting behind a gap. Each channel counts what
/// it holds, and a packet starts from the budget less all of it; a packet that needs more is
/// dropped without an ack, so its sender resends it once deliveries free room.
/// GameNetworkingSockets' `RecvBufferSize` and ENet's `maximumWaitingData` work the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReceiveBudget {
    left: usize,
}

/// The default receive budget, 1 MiB: twice the default `max_message_len`, which admits the
/// largest reassembly buffer a peer can describe.
pub(crate) const DEFAULT_RECEIVE_BUDGET: usize = 1024 * 1024;

impl ReceiveBudget {
    pub(crate) const fn new(left: usize) -> Self {
        Self { left }
    }

    /// Takes `bytes`, or refuses them when less is left.
    pub(crate) const fn try_charge(&mut self, bytes: usize) -> bool {
        if bytes > self.left {
            return false;
        }
        self.left -= bytes;
        true
    }

    /// Takes `bytes` even past the room left: for the message an ordered channel delivers next,
    /// which frees what waits behind it.
    pub(crate) const fn charge(&mut self, bytes: usize) {
        self.left = self.left.saturating_sub(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10 bytes of room take 6 then 4 and refuse 1 more; a forced charge empties the room.
    #[test]
    fn charges_up_to_the_room_left() {
        let mut budget = ReceiveBudget::new(10);
        assert!(budget.try_charge(6));
        assert!(budget.try_charge(4));
        assert!(!budget.try_charge(1));
        let mut budget = ReceiveBudget::new(3);
        budget.charge(5);
        assert!(!budget.try_charge(1));
        assert!(budget.try_charge(0));
    }
}
