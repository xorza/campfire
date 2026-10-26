use campfire_actions::{ActionSlots, SlotKind, SlotKindBook};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_script::UnknownName;
use campfire_values::{Named, UnitType};

use crate::uses_data::UsesData;

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

impl UseSlot {
    /// The use slot of each user type that stands, of its slot kind; an error for a kind the mode
    /// does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let kinds = &cx.book::<SlotKindBook>()?.0;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<UsesData>() else {
                continue;
            };
            let kind = kinds.names().named(data.kind.as_str()).ok_or_else(|| {
                let at = BookPlace::UnitType(unit.place());
                let name = UnknownName::new(SlotKind::NAMES, &data.kind);
                BookError::unknown(unit.package, at, name)
            })?;
            book.set(unit.unit_type, Self::new(kind));
        }
        Ok(book.build())
    }
}
