use std::ops::Range;

use campfire_math::Num;
use campfire_sim::StableId;

use crate::actions::action_book::{Action, ActionBook, Delivery};
use crate::actions::action_data;
use crate::actions::action_slots::ActionSlots;
use crate::units::action_id::ActionId;
use crate::units::filter::Filter;
use crate::units::script_view::{UnitRow, View};
use crate::units::view_column::ViewColumn;

/// What the action pipeline adds to the script view: the match's actions, and each unit's
/// ability slots, the unit its attacks aim at, and the range of its first weapon, a row each.
#[derive(Debug, Default)]
pub(crate) struct ActionsColumn {
    book: ActionBook,
    rows: Vec<ActionsRow>,
    slots: Vec<SlotRow>,
}

/// A unit's actions as the view read them: the unit its attacks aim at, the range of its first
/// weapon in meters, none with no weapon, and its run of ability slots.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionsRow {
    target: Option<StableId>,
    attack_range: Option<Num>,
    slots: Range<u32>,
}

/// An ability slot as the view read it: the rank of its ability, 0 while not learned, how many
/// ranks the ability has, and the filter of the units it may attack when it is a weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRow {
    pub(crate) rank: u8,
    pub(crate) ranks: u8,
    pub(crate) weapon: Option<Filter>,
}

impl ViewColumn for ActionsColumn {
    fn clear(&mut self) {
        self.rows.clear();
        self.slots.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl ActionsColumn {
    /// Adds the row of a unit with `slots`, or with none: each slot's rank, its action's ranks
    /// and its weapon filter, the attack target, and the range of its first weapon.
    pub(crate) fn push(&mut self, slots: Option<&ActionSlots>) {
        let start = u32::try_from(self.slots.len()).expect("slots fit u32");
        let Some(slots) = slots else {
            self.rows.push(ActionsRow {
                target: None,
                attack_range: None,
                slots: start..start,
            });
            return;
        };
        let book = &self.book;
        let attack_range = book.weapon_for(slots, None).map(|slot| {
            let action_data::Range::Meters(range) = book.range(slots, slot) else {
                panic!("the load gives every attack a range in meters");
            };
            range
        });
        self.slots.extend(slots.iter().map(|slot| {
            let action = book
                .get(slot.action)
                .expect("a slot's action is in the book");
            SlotRow {
                rank: slot.rank,
                ranks: u8::try_from(action.ranks.len()).expect("an action has few ranks"),
                weapon: action.weapon_filter(),
            }
        }));
        let end = u32::try_from(self.slots.len()).expect("slots fit u32");
        self.rows.push(ActionsRow {
            target: slots.attack_target(),
            attack_range,
            slots: start..end,
        });
    }

    /// Shares the match's actions, as the load built them, with the view, which names them to
    /// scripts.
    pub(crate) fn share(view: &View, book: ActionBook) {
        view.set_action_names(book.names());
        view.column_mut(|column: &mut ActionsColumn| {
            column.book = book;
        });
    }

    /// What `read` gives of the column, which every match's pipeline adds.
    fn read<R>(view: &View, read: impl FnOnce(&ActionsColumn) -> R) -> R {
        view.column(read)
            .expect("the action pipeline adds its column to every match's view")
    }

    /// The unit the attacks of the unit in row `row` aim at.
    pub(crate) fn target(view: &View, row: usize) -> Option<StableId> {
        ActionsColumn::read(view, |column| column.rows[row].target)
    }

    /// The range of the first weapon of the unit in row `row`, in meters; none with no weapon.
    pub(crate) fn attack_range(view: &View, row: usize) -> Option<Num> {
        ActionsColumn::read(view, |column| column.rows[row].attack_range)
    }

    /// How many ability slots the unit in row `row` has.
    pub(crate) fn slot_count(view: &View, row: usize) -> usize {
        ActionsColumn::read(view, |column| column.rows[row].slots.len())
    }

    /// Ability slot `slot` of the unit in row `row`, when it has one.
    pub(crate) fn slot(view: &View, row: usize, slot: u8) -> Option<SlotRow> {
        ActionsColumn::read(view, |column| {
            column.run(row).get(usize::from(slot)).copied()
        })
    }

    /// Whether a learned weapon of `unit`, in row `row`, selects `target`, as `unit` regards it.
    pub(crate) fn armed_against(view: &View, row: usize, unit: &UnitRow, target: &UnitRow) -> bool {
        let attitude = view.attitude(unit.team, target.team);
        let target = Some((attitude, target.tags.tags));
        ActionsColumn::read(view, |column| {
            column
                .run(row)
                .iter()
                .any(|slot| Action::arms(slot.rank, slot.weapon, target))
        })
    }

    /// How action `id` delivers, if other than at once.
    pub(crate) fn delivers(view: &View, id: ActionId) -> Option<Delivery> {
        ActionsColumn::read(view, |column| {
            column
                .book
                .get(id)
                .expect("an action of the match")
                .delivery
        })
    }

    /// The ability slots of the unit in row `row`.
    fn run(&self, row: usize) -> &[SlotRow] {
        let slots = &self.rows[row].slots;
        &self.slots[slots.start as usize..slots.end as usize]
    }
}
