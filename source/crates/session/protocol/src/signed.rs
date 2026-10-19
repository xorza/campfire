use crate::session_id::SessionId;

/// The bytes that start every message of one kind a key signs, `campfire/<kind>/v<n>`, so a
/// signature of one kind never counts as one of another, nor as a hash's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureDomain(&'static str);

impl SignatureDomain {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn as_bytes(self) -> &'static [u8] {
        self.0.as_bytes()
    }
}

/// A record of session `session_id`, as its server signs it: the session's id, then the record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InSession<'a, T> {
    pub session_id: SessionId,
    pub record: &'a T,
}

/// A record a key signs: the message is its kind's domain, then its body, as `write_message`
/// writes it, which no record changes.
pub trait Signed {
    const DOMAIN: SignatureDomain;

    /// Appends the record's body to `message`.
    fn write_body(&self, message: &mut Vec<u8>);
}

/// Writes the message a key signs of `record`, `domain ‖ body`, into `message`, in place of what
/// it held.
pub(crate) fn write_message<S: Signed>(record: &S, message: &mut Vec<u8>) {
    message.clear();
    message.extend_from_slice(S::DOMAIN.as_bytes());
    record.write_body(message);
}
