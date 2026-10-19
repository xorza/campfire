//! The stack's own UDP protocol: sealed sessions over an OS socket or the simulated network, made
//! by a TLS 1.3 handshake in the protocol's own packets, with a Retry for each new address, an
//! admission for each address, connection IDs and stateless resets. The decisions are in
//! (13-net-stack/udp.md).

mod address_token;
mod admission;
mod admission_config;
mod alert;
mod bind_error;
mod certificate_chain;
mod certificate_error;
mod crypto_stream;
mod derived_secret;
mod handshake_packet;
mod invalid_server_name;
mod pinned_key_verifier;
mod raw_key_signer;
mod recv_batch;
mod reset_key;
mod rustls_seal;
mod secure_establishment;
mod server_identity;
mod server_key;
mod server_secret;
mod server_trust;
mod sockets;
mod spki;
mod tls_provider;
mod transport_params;
mod udp_config;
mod udp_io;
mod udp_port;
mod used_tokens;

pub use crate::admission_config::AdmissionConfig;
pub use crate::bind_error::BindError;
pub use crate::certificate_chain::CertificateChain;
pub use crate::certificate_error::CertificateError;
pub use crate::invalid_server_name::InvalidServerName;
pub use crate::server_identity::ServerIdentity;
pub use crate::server_key::ServerKey;
pub use crate::server_secret::ServerSecret;
pub use crate::server_trust::ServerTrust;
pub use crate::sockets::{ClientSocket, EmulatedServer, ServerSocket, emulated_client};
pub use crate::udp_config::UdpConfig;
pub use crate::udp_io::UdpIo;

/// The crate's criterion cases, which `benches/net-udp.rs` runs.
#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::admission;

    /// Runs each case of the crate whose id criterion's filter takes.
    pub fn run(criterion: &mut Criterion) {
        campfire_measure::without_logs(criterion, admission::bench::admission);
    }
}
