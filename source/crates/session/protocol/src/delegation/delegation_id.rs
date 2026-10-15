use campfire_common::{Meaning, Value32};

/// A delegation's id: the id of its Nostr event, which a player's first input links to and a
/// receipt names.
pub type DelegationId = Value32<DelegationIdMeaning>;

/// The meaning of a `DelegationId`.
#[derive(Debug)]
pub enum DelegationIdMeaning {}

impl Meaning for DelegationIdMeaning {
    const NAME: &'static str = "DelegationId";
}
