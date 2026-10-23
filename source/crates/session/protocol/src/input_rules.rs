use campfire_common::Ticks;
use serde::{Deserialize, Serialize};

/// The limits of a session's terms on its inputs. They encode field by field, in the terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputRules {
    /// The most ticks an input may land after its stamp; a later one is logged as late.
    pub max_delay: Ticks,
    /// The most ticks an input's stamp may be ahead of the next tick; a further one is logged as
    /// early. The server holds each input until its tick, so this bounds what a client can make
    /// it hold.
    pub max_lead: Ticks,
    /// The most bytes an input's payload may hold.
    pub max_payload_len: u32,
    /// The most inputs of one stamp, and of one packet, a player may send, and the most of its
    /// inputs that apply in one tick, the rest waiting for the next. With the max payload length
    /// and the max lead, it bounds how fast a player can grow the log.
    pub max_per_tick: u32,
}

impl InputRules {
    /// The limits the reference server runs a session on a LAN by.
    pub const LAN: Self = Self {
        max_delay: Ticks::new(10),
        max_lead: Ticks::new(30),
        max_payload_len: 1024,
        max_per_tick: 4,
    };
}
