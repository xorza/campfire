use bytes::{Bytes, BytesMut};
use core::ops::Deref;

/// Byte payload received from a transport, in the form its IO backend holds it.
///
/// A session hands this payload to the frame once it opened it, and neither form costs it a copy.
/// Higher-level systems of the stack that read cheap immutable subslices take it with
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
mod tests {
    use super::*;

    #[test]
    fn either_receive_payload_freezes_without_a_copy() {
        let mutable = BytesMut::from(&[1, 2, 3][..]);
        let allocation = mutable.as_ptr();
        assert_eq!(ReceivePayload::from(mutable).freeze().as_ptr(), allocation);

        // A frozen payload whose allocation is shared stays a view of it.
        let bytes = Bytes::from(std::vec![1, 2, 3]);
        let shared = bytes.clone();
        assert_eq!(
            ReceivePayload::from(bytes).freeze().as_ptr(),
            shared.as_ptr()
        );
    }

    #[test]
    fn a_receive_payload_becomes_mutable_with_a_copy_only_when_frozen_and_shared() {
        let mutable = BytesMut::from(&[1, 2, 3][..]);
        let allocation = mutable.as_ptr();
        assert_eq!(
            ReceivePayload::from(mutable).into_mut().as_ptr(),
            allocation
        );

        let unique = Bytes::from(std::vec![1, 2, 3]);
        let allocation = unique.as_ptr();
        assert_eq!(ReceivePayload::from(unique).into_mut().as_ptr(), allocation);

        let bytes = Bytes::from(std::vec![1, 2, 3]);
        let shared = bytes.clone();
        let payload = ReceivePayload::from(bytes).into_mut();
        assert_ne!(payload.as_ptr(), shared.as_ptr());
        assert_eq!(payload.as_ref(), shared.as_ref());
    }

    #[test]
    fn receive_payloads_compare_by_their_bytes_in_either_form() {
        let mutable = ReceivePayload::from(BytesMut::from(&[1, 2, 3][..]));
        assert_eq!(
            mutable,
            ReceivePayload::from(Bytes::from_static(&[1, 2, 3]))
        );
        assert_ne!(
            mutable,
            ReceivePayload::from(Bytes::from_static(&[1, 2, 4]))
        );
    }
}
