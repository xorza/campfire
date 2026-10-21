use bytes::{Bytes, BytesMut};
use core::ops::Deref;

/// Byte payload received from a transport, in the form its IO backend holds it.
///
/// A packet session hands this payload to the frame once it opened it, and neither form costs it a
/// copy. Higher-level systems of the stack that read cheap immutable subslices take it with
/// [`freeze`](Self::freeze), which never copies; a layer that changes it in place takes it with
/// [`into_mut`](Self::into_mut), which copies only a frozen payload whose allocation is shared.
#[derive(Debug, Clone)]
pub enum ReceivePayload {
    /// A buffer the IO backend received into. A connection layer may change it in place, even
    /// while the backend's buffer pool holds another view of its allocation.
    Mutable(BytesMut),
    /// An immutable view the IO backend's API gave. It may share its allocation with other
    /// payloads, as datagrams received in one read do, or with the sender's buffer pool, as an
    /// in-process channel's do.
    Frozen(Bytes),
}

impl ReceivePayload {
    /// The payload as immutable bytes, with no copy.
    pub fn freeze(self) -> Bytes {
        match self {
            Self::Mutable(payload) => payload.freeze(),
            Self::Frozen(payload) => payload,
        }
    }

    /// The payload as mutable bytes: a frozen payload keeps its allocation when no other handle
    /// shares it, and is copied when one does.
    pub fn into_mut(self) -> BytesMut {
        match self {
            Self::Mutable(payload) => payload,
            Self::Frozen(payload) => BytesMut::from(payload),
        }
    }
}

impl Deref for ReceivePayload {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Mutable(payload) => payload,
            Self::Frozen(payload) => payload,
        }
    }
}

/// Two payloads are equal when their bytes are, whichever form each is in.
impl PartialEq for ReceivePayload {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for ReceivePayload {}

impl From<BytesMut> for ReceivePayload {
    fn from(payload: BytesMut) -> Self {
        Self::Mutable(payload)
    }
}

impl From<Bytes> for ReceivePayload {
    fn from(payload: Bytes) -> Self {
        Self::Frozen(payload)
    }
}

/// The bytes of one message the frame sends.
pub type SendPayload = Bytes;

#[cfg(test)]
mod tests;
