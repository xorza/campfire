use std::net::SocketAddr;
use std::num::NonZeroU32;

use campfire_common::TlsKey;
use campfire_log::{LogEvent, LogLine};
use secp256k1::XOnlyPublicKey;
use serde::Deserialize;
use tracing::info;

/// The server listens on its socket's address: players join with its TLS key, key and tick
/// rate, by the command in `join`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Listening {
    #[serde(deserialize_with = "LogLine::text")]
    pub address: SocketAddr,
    #[serde(deserialize_with = "LogLine::text")]
    pub tls_key: TlsKey,
    #[serde(deserialize_with = "LogLine::text")]
    pub server_key: XOnlyPublicKey,
    pub tick_hz: NonZeroU32,
    pub join: String,
}

impl LogEvent for Listening {
    const MESSAGE: &'static str = "listening; players join with the command in `join`";

    fn log(&self) {
        info!(
            address = %self.address,
            tls_key = %self.tls_key,
            server_key = %self.server_key,
            tick_hz = self.tick_hz.get(),
            join = self.join,
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
        // The x coordinate of secp256k1's generator: a valid key.
        let key = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        round_trip(&Listening {
            address: "0.0.0.0:4433".parse().unwrap(),
            tls_key: TlsKey::new([7; 32]),
            server_key: key.parse().unwrap(),
            tick_hz: NonZeroU32::new(30).unwrap(),
            join: "campfire-client …".to_owned(),
        });
    }
}
