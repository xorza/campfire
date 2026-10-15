use campfire_values::NameId;
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

    pub const fn new(index: u8) -> Layer {
        Layer(index)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl NameId for Layer {
    type Scope = ();
    const LIMIT: usize = 1 << u8::BITS;

    fn at(index: usize) -> Layer {
        Layer(u8::try_from(index).expect("layers fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}
