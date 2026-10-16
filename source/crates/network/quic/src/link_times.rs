use std::time::Duration;

use quinn_proto::{IdleTimeout, TransportConfig, VarInt};

/// How often a quiet connection proves it lives, and how long a silent one lasts before it is
/// lost. Both ends' idle timeouts meet at the lower one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkTimes {
    pub keep_alive: Duration,
    pub idle_timeout: Duration,
}

impl LinkTimes {
    /// A keep-alive each second and a loss after 5 s (design 15, D8).
    pub const CAMPFIRE: Self = Self {
        keep_alive: Duration::from_secs(1),
        idle_timeout: Duration::from_secs(5),
    };

    /// The transport parameters of every connection: these times, and no stream either way, as a
    /// link carries datagrams alone and a stream's data would wait unread (D4).
    pub(crate) fn transport(self) -> TransportConfig {
        assert!(
            self.keep_alive < self.idle_timeout,
            "a keep-alive less often than the idle timeout lets a quiet link time out"
        );
        let mut transport = TransportConfig::default();
        transport
            .max_concurrent_bidi_streams(VarInt::from_u32(0))
            .max_concurrent_uni_streams(VarInt::from_u32(0))
            .keep_alive_interval(Some(self.keep_alive))
            .max_idle_timeout(Some(
                IdleTimeout::try_from(self.idle_timeout).expect("an idle timeout fits a varint"),
            ));
        transport
    }
}
