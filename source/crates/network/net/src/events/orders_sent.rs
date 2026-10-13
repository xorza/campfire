use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::debug;

/// The client of `slot` sent `orders` orders in one message, each stamped `stamp`, at chain seqs
/// from `seq` on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct OrdersSent {
    pub slot: PlayerSlot,
    pub seq: u64,
    pub stamp: Tick,
    pub orders: usize,
}

impl LogEvent for OrdersSent {
    const MESSAGE: &'static str = "sent orders";

    fn log(&self) {
        debug!(
            slot = self.slot.get(),
            seq = self.seq,
            stamp = self.stamp.get(),
            orders = self.orders,
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&OrdersSent {
            slot: PlayerSlot::new(1),
            seq: 7,
            stamp: Tick::new(20),
            orders: 2,
        });
    }
}
