use campfire_actions::{ActionSlots, SlotKind};
use campfire_declarations::PerId;
use campfire_values::UnitType;

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

    /// `slots` with the one empty slot of its kind, after the slots of that kind, as a user's are
    /// as it spawns.
    pub fn extend_slots(self, slots: ActionSlots) -> ActionSlots {
        slots.with_empty(self.0, 1)
    }
}

impl PerId<UnitType> for UseSlot {
    const SCRIPTS_ONLY: bool = false;
}
