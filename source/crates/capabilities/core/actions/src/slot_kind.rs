use campfire_core::{NameKind, name_script_arg};
use campfire_values::NameId;
use serde::{Deserialize, Serialize};

/// A slot kind of the mode: its place in the mode's `[[slots]]`. A unit's slots go kind after
/// kind in that order, which is how a client binds its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotKind(u8);

impl SlotKind {
    pub const fn new(index: u8) -> SlotKind {
        SlotKind(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for SlotKind {
    type Scope = ();
    const LIMIT: usize = 1 << u8::BITS;

    fn at(index: usize) -> SlotKind {
        SlotKind(u8::try_from(index).expect("slot kinds fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(SlotKind, NameKind::SlotKind, |ctx, name| {
    ctx.view().names().table::<SlotKind>()?.named(name)
});
