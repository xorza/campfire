use serde::{Deserialize, Serialize};

/// A layer bodies move on: its index in the mode's `[navigation] layers`, the first the default.
/// Collision and pathing work within a layer, so an air unit passes over ground units and walls.
/// Each layer's name is a tag, so a match's tag limit bounds the layers too.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Layer(u8);

impl Layer {
    pub const FIRST: Layer = Layer(0);

    pub(crate) const fn new(index: u8) -> Layer {
        Layer(index)
    }

    pub(crate) const fn index(self) -> u8 {
        self.0
    }
}
