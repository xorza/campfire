use campfire_common::{Meaning, Value32};

/// A player's main key: the x-only public key of their Nostr identity, which signs their
/// delegations, as its 32 bytes.
pub type MainKey = Value32<MainKeyMeaning>;

/// The meaning of a `MainKey`.
#[derive(Debug)]
pub enum MainKeyMeaning {}

impl Meaning for MainKeyMeaning {
    const NAME: &'static str = "MainKey";
}
