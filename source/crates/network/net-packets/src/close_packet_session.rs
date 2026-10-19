use campfire_net_payload::AppCode;

use crate::endpoint_core::PeerKey;

/// Closes a packet session of an endpoint of any IO, as a host's handle to one packet session holds
/// it (13-net-stack/packet-session.md, A close carries its code).
pub trait ClosePacketSession: Send + Sync + core::fmt::Debug {
    /// Closes packet session `key` with `code`.
    fn close(&self, key: PeerKey, code: AppCode);
}
