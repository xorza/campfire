//! The client's end of a local link, a bundle of its own.
use bevy_ecs::bundle::Bundle;
use campfire_net_endpoint::Endpoint;
use campfire_net_packets::EmulatedWires;

/// The client's end of a local link: its endpoint of the sealed link over the emulator, which the
/// client inserts on its entity (08-sessions.md, One in-process link).
#[derive(Bundle, Debug)]
pub struct LocalLink {
    endpoint: Endpoint<EmulatedWires>,
}

impl LocalLink {
    pub(crate) const fn new(endpoint: Endpoint<EmulatedWires>) -> Self {
        Self { endpoint }
    }
}
