use campfire_net_payload::AppCode;

use crate::endpoint_core::PeerKey;

/// Closes a session of an endpoint of any IO, as a host's handle to one session holds it.
pub trait CloseSession: Send + Sync + core::fmt::Debug {
    /// Closes session `key` with `code`.
    fn close(&self, key: PeerKey, code: AppCode);
}
