use campfire_math::Num;
use core::time::Duration;

use crate::link_model::{LinkModel, per_mille};
use crate::rate_limit::RateLimit;

/// The models of both directions of an `EmulatedIo` link (13-net-stack/testing.md, Presets are
/// plausible profiles).
///
/// The presets other than [`perfect`](Self::perfect) take the Network Link Conditioner's values as
/// third parties report them (countingup, NSHipster), downlink as server to client; each queue
/// holds one second at its link's rate. They are plausible test profiles, not measurements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DuplexModel {
    pub client_to_server: LinkModel,
    pub server_to_client: LinkModel,
    /// The largest packet the link carries each way, in bytes; a larger one is dropped, in every
    /// phase. `None` carries any size. The packet session sizes its payloads by it.
    pub mtu: Option<usize>,
    /// The seed each direction's draws follow, indexed by
    /// [`LinkDirection::index`](campfire_net_core::Direction::index): two runs of one model on the
    /// same packet sequence condition it identically.
    pub seeds: [u64; 2],
}

impl DuplexModel {
    /// Both directions deliver every packet at once.
    pub fn perfect() -> Self {
        Self::default()
    }

    /// Both directions follow `model`.
    pub fn symmetric(model: LinkModel) -> Self {
        Self {
            client_to_server: model.clone(),
            server_to_client: model,
            ..Self::default()
        }
    }

    /// LTE: 50 Mbit/s each way, 65 ms up and 50 ms down, no loss.
    pub fn lte() -> Self {
        Self::preset(
            OneWay::new(6_250_000, 65),
            OneWay::new(6_250_000, 50),
            Num::ZERO,
        )
    }

    /// EDGE: 200 kbit/s and 440 ms up, 240 kbit/s and 400 ms down, no loss.
    pub fn edge() -> Self {
        Self::preset(
            OneWay::new(25_000, 440),
            OneWay::new(30_000, 400),
            Num::ZERO,
        )
    }

    /// Very Bad Network: 1 Mbit/s and 500 ms each way, 10 % loss each way.
    pub fn very_bad() -> Self {
        Self::preset(
            OneWay::new(125_000, 500),
            OneWay::new(125_000, 500),
            per_mille(100),
        )
    }

    fn preset(up: OneWay, down: OneWay, loss: Num) -> Self {
        let one_way = |direction: OneWay| {
            LinkModel::perfect()
                .with_latency(Duration::from_millis(direction.millis))
                .with_rate(RateLimit::new(
                    direction.bytes_per_second,
                    direction.bytes_per_second,
                ))
                .with_fixed_loss(loss)
        };
        Self {
            client_to_server: one_way(up),
            server_to_client: one_way(down),
            ..Self::default()
        }
    }

    /// Both directions with their seeds replaced, so two links of one preset draw apart.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seeds = [seed, seed ^ 0x9E37_79B9_7F4A_7C15];
        self
    }

    /// Replaces [`mtu`](Self::mtu).
    #[must_use]
    pub const fn with_mtu(mut self, mtu: usize) -> Self {
        self.mtu = Some(mtu);
        self
    }
}

/// One direction of a preset: its rate, whose queue holds one second, and its latency.
#[derive(Debug, Clone, Copy)]
struct OneWay {
    bytes_per_second: u32,
    millis: u64,
}

impl OneWay {
    const fn new(bytes_per_second: u32, millis: u64) -> Self {
        Self {
            bytes_per_second,
            millis,
        }
    }
}
