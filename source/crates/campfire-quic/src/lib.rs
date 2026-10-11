//! QUIC alone under Lightyear's `Link` (design 15): an endpoint driven by a thread of its own,
//! whose connections carry each Lightyear packet as one DATAGRAM frame, a server that presents a
//! raw Ed25519 key, and a client that takes the key it pinned and no other. It knows no game:
//! `net` installs it.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems and hooks take their parameters by value"
)]

mod app_code;
mod close_link;
mod derived_secret;
mod endpoint_state;
mod error;
mod io_thread;
mod link_end;
mod link_times;
mod outbox;
mod pinned_key_verifier;
mod quic_endpoint;
mod quic_link;
mod quic_plugin;
mod recv_batch;
mod reset_key;
mod server_key;
mod shared_endpoint;
mod spki;
mod udp_port;

pub use crate::app_code::AppCode;
pub use crate::close_link::CloseLink;
pub use crate::error::BindError;
pub use crate::link_end::LinkEnd;
pub use crate::link_times::LinkTimes;
pub use crate::quic_endpoint::QuicEndpoint;
pub use crate::quic_plugin::QuicPlugin;
pub use crate::reset_key::ResetKey;
pub use crate::server_key::ServerKey;

/// The minimum MTU of every Campfire link, which both its ends' fragments take: the largest
/// DATAGRAM frame on QUIC's minimum path of 1,200 bytes, `1200 − (1 + 8 + 4 + 16) − 9`, a short
/// header with connection ids of 8 bytes, a packet number of at most 4 and an AEAD tag of 16, and
/// the frame's type and largest length (design 15, D5).
pub const LINK_MIN_MTU: usize = 1162;
