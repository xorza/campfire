use campfire_common::{Meaning, Value32};

/// A session's id: the hash of its terms, see `SessionTerms::session_id`. Delegations and
/// chain-head signatures name it, so neither counts in another session, and both sign the terms.
pub type SessionId = Value32<SessionIdMeaning>;

/// The meaning of a `SessionId`.
#[derive(Debug)]
pub enum SessionIdMeaning {}

impl Meaning for SessionIdMeaning {
    const NAME: &'static str = "SessionId";
}
