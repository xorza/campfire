//! The client's end of a local link, a bundle of its own.
#![expect(
    clippy::mem_forget,
    reason = "Bevy's `Bundle` derive moves the fields out through `deconstruct_moving_ptr!`, which forgets the bundle"
)]

use std::fmt;

use bevy_ecs::bundle::Bundle;
use campfire_quic::LinkBinding;
use lightyear::crossbeam::CrossbeamIo;

/// The client's end of a local link and the binding both ends take, which the client inserts on
/// its entity as one, so no client holds the link with no binding.
#[derive(Bundle)]
pub struct LocalLink {
    io: CrossbeamIo,
    binding: LinkBinding,
}

impl LocalLink {
    pub(crate) const fn new(io: CrossbeamIo, binding: LinkBinding) -> Self {
        Self { io, binding }
    }
}

impl fmt::Debug for LocalLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalLink")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}
