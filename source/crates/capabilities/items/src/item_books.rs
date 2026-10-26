use std::mem;

use campfire_actions::{ActionSlots, UseGroup};
use campfire_declarations::{BookCx, BookError, ById};
use campfire_players::ResourceBook;
use campfire_values::{Names, ResourceId, UnitType};

use crate::Items;
use crate::inventory::Inventory;

impl Items {
    /// Adds the empty slots of each inventory to its unit type's slots, of its slot kind.
    pub(crate) fn inventory_slots(
        cx: &BookCx<'_>,
        slots: &mut ById<UnitType, ActionSlots>,
    ) -> Result<(), BookError> {
        let inventories = cx.book::<ById<UnitType, Inventory>>()?;
        let groups = cx.book::<ById<UnitType, UseGroup>>()?;
        let mut built = mem::take(slots).reopen();
        for unit in cx.standing() {
            let (Some(inventory), Some(&group)) =
                (inventories.get(unit.unit_type), groups.get(unit.unit_type))
            else {
                continue;
            };
            let held = built.take(unit.unit_type);
            let held = held.unwrap_or_else(|| ActionSlots::new([]));
            built.set(unit.unit_type, inventory.extend_slots(held, group));
        }
        *slots = built.build();
        Ok(())
    }

    /// The names of the players' resources, none in a match without them.
    pub(crate) fn resources<'c>(
        cx: &'c BookCx<'_>,
        none: &'c Names<ResourceId>,
    ) -> Result<&'c Names<ResourceId>, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        Ok(resources.map_or(none, |resources| &*resources.0))
    }
}
