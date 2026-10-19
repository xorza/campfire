use campfire_script::name_script_arg;
use campfire_values::{Article, NameId, NameKind, NameScope, Named};
use serde::{Deserialize, Serialize};

/// A slot kind of the mode: its place in the mode's `[[slots]]`. A unit's slots go kind after
/// kind in that order, which is how a client binds its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotKind(u8);

impl SlotKind {
    pub const fn new(index: u8) -> Self {
        Self(index)
    }
}

impl NameId for SlotKind {
    type Scope = ();
    const LIMIT: usize = 1 << u8::BITS;

    fn at(index: usize) -> Self {
        Self(u8::try_from(index).expect("slot kinds fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(SlotKind, |ctx, name| {
    ctx.view().names().table::<SlotKind>()?.named(name)
});

impl Named for SlotKind {
    const NAMES: NameKind = NameKind::new("slot kind", Article::A, NameScope::Match);
}
