use campfire_actions::SlotKind;
use campfire_declare::PerType;

/// A user type's use slot, package data and not state: the slot kind of the one empty slot its
/// units spawn with, which a use fills with its object's action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UseSlot(SlotKind);

impl UseSlot {
    pub const fn new(kind: SlotKind) -> Self {
        Self(kind)
    }

    pub const fn kind(self) -> SlotKind {
        self.0
    }
}

impl PerType for UseSlot {
    const SCRIPTS_ONLY: bool = false;
}
