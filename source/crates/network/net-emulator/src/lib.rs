//! The network model and the emulated network: a [`LinkModel`] of latency, jitter, loss,
//! duplication, reordering and rate in netem's terms; the [`LinkConditioner`] that applies one to
//! what an endpoint receives; and a [`EmulatedNetwork`], whose [`EmulatedIo`] wires each follow a
//! seeded model in each direction, on the network's own wire clock (13-net-stack/testing.md, One
//! `LinkModel` for emulated and real links, in netem's terms).

#[cfg(feature = "internals")]
pub mod bench;
mod direction_counts;
mod duplex_model;
mod emulated_direction;
mod emulated_link_id;
mod emulated_network;
mod link_conditioner;
mod link_conditioning;
mod link_model;
mod link_model_run;
mod loss_state;
mod phase;
mod phase_mode;
mod rate_limit;
mod ready_buffer;
mod side;
mod wire_clock;

pub use crate::direction_counts::DirectionCounts;
pub use crate::duplex_model::DuplexModel;
pub use crate::emulated_link_id::EmulatedLinkId;
pub use crate::emulated_network::EmulatedIo;
pub use crate::emulated_network::EmulatedNetwork;
pub use crate::emulated_network::EmulatedPair;
pub use crate::link_conditioner::LinkConditioner;
pub use crate::link_conditioning::LinkConditioning;
pub use crate::link_model::LinkModel;
pub use crate::phase::Phase;
pub use crate::phase_mode::PhaseMode;
pub use crate::rate_limit::RateLimit;
pub use crate::side::Side;
pub use crate::wire_clock::WireClock;
