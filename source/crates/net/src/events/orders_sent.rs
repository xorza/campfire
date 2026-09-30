use campfire_log::LogEvent;
use campfire_sim::Tick;
use serde::Deserialize;
use tracing::debug;

/// A client sent `orders` orders in one message, each stamped `stamp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct OrdersSent {
    pub stamp: Tick,
    pub orders: usize,
}

impl LogEvent for OrdersSent {
    const MESSAGE: &'static str = "sent orders";

    fn log(&self) {
        debug!(
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
            stamp: Tick::new(20),
            orders: 2,
        });
    }
}
