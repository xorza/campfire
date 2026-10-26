use std::mem;

use campfire_actions::ActionSlots;
use campfire_declarations::{BookCx, BookError, ById};
use campfire_values::UnitType;

use crate::Interaction;
use crate::use_slot::UseSlot;

impl Interaction {
    /// Adds each use slot to its unit type's slots, empty, of its slot kind.
    pub(crate) fn use_slots(
        cx: &BookCx<'_>,
        slots: &mut ById<UnitType, ActionSlots>,
    ) -> Result<(), BookError> {
        let uses = cx.book::<ById<UnitType, UseSlot>>()?;
        let mut built = mem::take(slots).reopen();
        for unit in cx.standing() {
            let Some(uses) = uses.get(unit.unit_type) else {
                continue;
            };
            let held = built.take(unit.unit_type);
            let held = held.unwrap_or_else(|| ActionSlots::new([]));
            built.set(unit.unit_type, uses.extend_slots(held));
        }
        *slots = built.build();
        Ok(())
    }
}
