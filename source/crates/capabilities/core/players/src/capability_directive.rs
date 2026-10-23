use campfire_codec::{Binary, Decoded};
use campfire_sim::Capability;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The directive of one capability's orders, the body of each command to that capability; the
/// units it commands are its player's (commanded-units.md, A player's orders go to its commanded
/// units).
pub trait CapabilityDirective: Serialize + DeserializeOwned + Copy + Send + Sync + 'static {
    /// The capability whose commands hold it.
    const CAPABILITY: Capability;

    /// The directive `body` holds; `None` unless the body is exactly one directive: a client can
    /// send any bytes.
    fn decode(body: &[u8]) -> Option<Self> {
        let Decoded { value, rest } = Binary::take::<Self>(body).ok()?;
        rest.is_empty().then_some(value)
    }
}
