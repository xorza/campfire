//! QUIC alone under Lightyear's `Link` (design 15): an endpoint driven by a thread of its own,
//! whose connections carry each Lightyear packet as one DATAGRAM frame, a server that presents a
//! raw Ed25519 key, and a client that takes the key it pinned and no other. It knows no game:
//! `net` installs it.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems and hooks take their parameters by value"
)]

mod address_limit;
mod admission;
mod app_code;
mod arena_ring;
mod close_link;
mod derived_secret;
mod endpoint_state;
mod error;
mod io_thread;
mod link_binding;
mod link_end;
mod link_times;
mod outbox;
mod peer;
mod pinned_key_verifier;
mod port_map;
mod quic_endpoint;
mod quic_link;
mod quic_plugin;
mod recv_batch;
mod reset_key;
mod server_key;
mod shared_endpoint;
mod spki;
mod udp_port;
mod used_tokens;

pub use crate::address_limit::AddressLimit;
pub use crate::app_code::AppCode;
pub use crate::close_link::CloseLink;
pub use crate::error::BindError;
pub use crate::link_binding::LinkBinding;
pub use crate::link_end::LinkEnd;
pub use crate::link_times::LinkTimes;
pub use crate::port_map::PortMap;
pub use crate::quic_endpoint::{QuicEndpoint, ServerSocket};
pub use crate::quic_plugin::QuicPlugin;

/// The length of every connection id Campfire's endpoints give, which `LINK_MIN_MTU` counts
/// (design 15, D5).
const CID_LEN: usize = 8;

/// The minimum MTU of every Campfire link, which both its ends' fragments take: the largest
/// DATAGRAM frame on QUIC's minimum path of 1,200 bytes, less a short header of one byte, the
/// connection id and a packet number of at most 4 bytes, an AEAD tag of 16, and the frame's type
/// and largest length varint (design 15, D5).
pub const LINK_MIN_MTU: usize = 1200 - (1 + CID_LEN + 4 + 16) - (1 + 8);

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::quic_endpoint::thread_fixture::ThreadFixture;
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::admission;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        admission::bench::admission(c);
    }
}
