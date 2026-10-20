//! The stack's packet session: one numbered, acknowledged and, on a network, sealed packet stream
//! for each link, under the transport's messages (11-net-stack/packet-session.md, Per-packet state
//! is the packet session's, per-message state the transport's).

mod bandwidth_limit;
mod bandwidth_limiter;
mod carrier;
mod close_packet_session;
mod connection_id;
mod datagram_arena;
mod datagram_io;
mod emulated_wires;
mod endpoint_core;
mod endpoint_pieces;
mod endpoint_runtime;
mod endpoint_setup;
mod establish;
#[cfg(feature = "internals")]
mod fixtures;
mod header;
mod header_error;
mod io_failed;
mod new_reno;
mod open_error;
mod packet_numbers;
mod packet_session;
mod packet_session_setup;
mod plain_establishment;
mod protection;
mod received_window;
mod rtt_estimator;
mod seal;
mod sent_packets;
mod signal;
mod stateless_reset;
mod transmit;

pub use crate::bandwidth_limit::BandwidthLimit;
pub use crate::carrier::Carrier;
pub use crate::close_packet_session::ClosePacketSession;
pub use crate::connection_id::ConnectionId;
pub use crate::datagram_arena::datagram_arena;
pub use crate::datagram_io::{Datagram, DatagramIo, Outbound};
pub use crate::emulated_wires::EmulatedWires;
pub use crate::endpoint_core::{EndpointCore, EndpointTime, News, PeerKey};
pub use crate::endpoint_pieces::EndpointPieces;
pub use crate::endpoint_runtime::{EndpointRuntime, PassScratch};
pub use crate::endpoint_setup::EndpointSetup;
pub use crate::establish::{
    ClientEstablish, Establish, Established, Establishing, ServerEstablish,
};
pub use crate::io_failed::IoFailed;
pub use crate::open_error::OpenError;
pub use crate::packet_session_setup::PacketSessionSetup;
pub use crate::protection::Protection;
pub use crate::seal::{Seal, TAG_LEN};
pub use crate::signal::Signal;
pub use crate::stateless_reset::{MIN_LEN as MIN_RESET_LEN, TOKEN_LEN as RESET_TOKEN_LEN};

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::plain_host::{CLIENT, HostLink, PlainHost};
    pub use crate::packet_session::internals::PacketSessionCounts;
}
