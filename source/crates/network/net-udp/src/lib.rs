//! The stack's own UDP protocol: sealed packet sessions over an OS socket or the emulated network,
//! made by a TLS 1.3 handshake in the protocol's own packets, with a Retry for each new address, an
//! admission for each address, connection IDs and stateless resets. The decisions are in
//! (11-net-stack/udp.md).

mod address_token;
mod admission;
mod admission_setup;
mod alert;
mod certificate_chain;
mod certificate_error;
mod crypto_stream;
mod derived_secret;
mod handshake_packet;
mod invalid_server_name;
mod pinned_key_verifier;
mod raw_key_signer;
mod reset_key;
mod rustls_seal;
mod secure_establishment;
mod server_identity;
mod server_secret;
mod server_trust;
mod spki;
#[cfg(feature = "internals")]
mod test_link;
mod tls_provider;
mod transport_params;
mod udp_client;
mod udp_server;
mod udp_setup;
mod used_tokens;

pub use crate::admission_setup::AdmissionSetup;
pub use crate::certificate_chain::CertificateChain;
pub use crate::certificate_error::CertificateError;
pub use crate::invalid_server_name::InvalidServerName;
pub use crate::server_identity::ServerIdentity;
pub use crate::server_secret::ServerSecret;
pub use crate::server_trust::ServerTrust;
pub use crate::udp_client::UdpClient;
pub use crate::udp_server::UdpServer;
pub use crate::udp_setup::UdpSetup;

/// The protocol's test link, which the tests of its sockets share.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::test_link::TestLink;
}

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
