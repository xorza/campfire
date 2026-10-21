//! How a TLS alert of a failed handshake ends the client's link.

use campfire_net_payload::UnlinkReason;

/// The alerts a client sends when it refuses the server's identity (RFC 8446 §6): for a
/// certificate, `bad_certificate` (42), `unsupported_certificate` (43), `certificate_revoked`
/// (44), `certificate_expired` (45), `certificate_unknown` (46) and `unknown_ca` (48); for a raw
/// key the client did not pin, `access_denied` (49). `illegal_parameter` (47) is no identity's.
const IDENTITY_ALERTS: [u8; 7] = [42, 43, 44, 45, 46, 48, 49];

/// How a handshake that failed with `alert`, sent or received, ends the client's link: a refused
/// identity is `WrongTlsKey`, and any other alert the transport error of its code, 0x100 + the
/// alert, as QUIC numbers it (RFC 9001 §4.8).
pub(crate) fn unlink_reason(alert: u8) -> UnlinkReason {
    if IDENTITY_ALERTS.contains(&alert) {
        UnlinkReason::WrongTlsKey
    } else {
        UnlinkReason::Transport {
            code: 0x100 + u64::from(alert),
        }
    }
}

#[cfg(test)]
mod tests;
