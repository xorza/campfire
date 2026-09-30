/// A session's id, which the server chooses. Delegations and chain-head signatures name it, so
/// neither counts in another session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId([u8; 32]);

impl SessionId {
    pub const fn new(bytes: [u8; 32]) -> SessionId {
        SessionId(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
