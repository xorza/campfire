//! Sealed packet sessions by a TLS 1.3 handshake in the IO's own packets (13-net-stack/udp.md, The
//! handshake is TLS 1.3 by rustls's QUIC API).

use campfire_net_packets::ChannelBinding;
use rustls::quic::Connection;

pub(crate) mod client_side;
mod resend_timer;
pub(crate) mod server_side;

/// RFC 9266's `tls-exporter` binding of `tls`'s completed handshake, which both ends read alike.
pub(crate) fn binding(tls: &Connection) -> ChannelBinding {
    let bytes = tls
        .export_keying_material([0; 32], b"EXPORTER-Channel-Binding", Some(b""))
        .expect("a completed TLS 1.3 handshake exports 32 bytes");
    ChannelBinding(bytes)
}

#[cfg(test)]
mod tests;
