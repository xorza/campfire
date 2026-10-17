use campfire_actions::{ActionBook, ActionSlots, SlotKindBook};
use campfire_declare::{BookCx, BookError, BookPlace, ByType};
use campfire_script::{Filter, NameKind, UnknownName};
use campfire_units::UnitTypes;

use crate::Interaction;
use crate::use_data::UseData;
use crate::use_slot::UseSlot;
use crate::use_spec::UseSpec;
use crate::uses_data::UsesData;

impl Interaction {
    /// Adds each use slot to its unit type's slots, empty, of its slot kind.
    pub(crate) fn use_slots(
        cx: &BookCx<'_>,
        slots: &mut ByType<ActionSlots>,
    ) -> Result<(), BookError> {
        let uses = cx.book::<ByType<UseSlot>>()?;
        for unit in cx.standing() {
            let Some(uses) = uses.get(unit.unit_type) else {
                continue;
            };
            let held = slots.take(unit.unit_type);
            let held = held.unwrap_or_else(|| ActionSlots::new([]));
            slots.set(unit.unit_type, held.with_empty(uses.kind(), 1));
        }
        Ok(())
    }
}

impl UseSpec {
    /// The use of each object type that stands, its action among its package's and its users
    /// among the match's tags; an error for a name the package or the match does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<UseData>() else {
                continue;
            };
            let spec = || {
                let action = actions.names().named((unit.package, data.action.as_str()));
                let action =
                    action.ok_or_else(|| UnknownName::new(NameKind::Ability, &data.action))?;
                let users = Filter::resolve(&data.users, tags)?;
                Ok(Self::new(action, users))
            };
            let at = || BookPlace::UnitType(unit.place());
            let spec = spec().map_err(|name| BookError::unknown(unit.package, at(), name))?;
            book.set(unit.unit_type, spec);
        }
        Ok(book)
    }
}

impl UseSlot {
    /// The use slot of each user type that stands, of its slot kind; an error for a kind the mode
    /// does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let kinds = &cx.book::<SlotKindBook>()?.0;
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<UsesData>() else {
                continue;
            };
            let kind = kinds.names().named(data.kind.as_str()).ok_or_else(|| {
                let at = BookPlace::UnitType(unit.place());
                let name = UnknownName::new(NameKind::SlotKind, &data.kind);
                BookError::unknown(unit.package, at, name)
            })?;
            book.set(unit.unit_type, Self::new(kind));
        }
        Ok(book)
    }
}
