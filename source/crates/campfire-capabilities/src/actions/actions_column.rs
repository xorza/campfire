use std::ops::Range;

use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::actions::action::Action;
use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::delivery::Delivery;
use crate::actions::range;
use crate::actions::slot_kind::SlotKind;
use crate::actions::slot_kinds::SlotKinds;
use crate::scripts::error::{ApiError, Checked};
use crate::units::action_id::ActionId;
use crate::units::filter::Filter;
use crate::units::kept_rows::{ColumnRows, KeptRows, RunMove};
use crate::units::script_view::View;
use crate::units::unit_row::UnitRow;
use crate::units::view_column::ViewColumn;

/// What the action pipeline adds to the script view: the match's actions and slot kinds, and each
/// unit's ability slots, the unit its attacks aim at, and the range of its first weapon, a row
/// each.
#[derive(Debug, Default)]
pub(crate) struct ActionsColumn {
    book: ActionBook,
    kinds: SlotKinds,
    rows: KeptRows<ActionsRows>,
}

/// The rows of one read of the actions column.
#[derive(Debug, Default, PartialEq, Eq)]
struct ActionsRows {
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

/// An ability slot as the view read it: its action, the rank of its ability, 0 while not learned,
/// how many ranks the ability has, and the filter of the units it may attack when it is a weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRow {
    pub(crate) action: Option<ActionId>,
    pub(crate) rank: u8,
    pub(crate) ranks: u8,
    pub(crate) weapon: Option<Filter>,
}

impl ViewColumn for ActionsColumn {
    fn begin(&mut self, _: &World) -> bool {
        self.rows.begin();
        false
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.rows.keep(rows);
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn same_as_kept(&self) -> bool {
        self.rows.same_as_kept()
    }
}

impl ColumnRows for ActionsRows {
    fn clear(&mut self) {
        self.rows.clear();
        self.slots.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        let (first, end) = (
            from.rows[rows.start].slots.start,
            from.rows[rows.end - 1].slots.end,
        );
        let moved = RunMove::new(first, self.slots.len());
        self.rows
            .extend(from.rows[rows].iter().map(|row| ActionsRow {
                target: row.target,
                attack_range: row.attack_range,
                slots: moved.of(&row.slots),
            }));
        self.slots
            .extend_from_slice(&from.slots[first as usize..end as usize]);
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

impl ActionsRows {
    /// The run of slots of the unit in row `row`.
    fn slots(&self, row: usize) -> &[SlotRow] {
        let slots = &self.rows[row].slots;
        &self.slots[slots.start as usize..slots.end as usize]
    }
}

impl ActionsColumn {
    /// Adds the row of a unit with `slots`, or with none: each slot's rank, its action's ranks
    /// and its weapon filter, the attack target, and the range of its first weapon.
    pub(crate) fn push(&mut self, slots: Option<&ActionSlots>) {
        let (book, rows) = (&self.book, self.rows.now_mut());
        let start = u32::try_from(rows.slots.len()).expect("slots fit u32");
        let Some(slots) = slots else {
            rows.rows.push(ActionsRow {
                target: None,
                attack_range: None,
                slots: start..start,
            });
            return;
        };
        let attack_range = book.weapon_for(slots, None).map(|slot| {
            let range::Range::Meters(range) = book.range(slots, slot) else {
                panic!("the load gives every attack a range in meters");
            };
            range
        });
        rows.slots.extend(slots.iter().map(|slot| {
            let action = slot
                .action
                .map(|action| book.get(action).expect("a slot's action is in the book"));
            SlotRow {
                action: slot.action,
                rank: slot.rank,
                ranks: action.map_or(0, |action| {
                    u8::try_from(action.ranks.len()).expect("an action has few ranks")
                }),
                weapon: action.and_then(Action::weapon_filter),
            }
        }));
        let end = u32::try_from(rows.slots.len()).expect("slots fit u32");
        rows.rows.push(ActionsRow {
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

    /// Shares the mode's slot kinds with the view, which names them to scripts.
    pub(crate) fn share_kinds(view: &View, kinds: SlotKinds) {
        view.column_mut(|column: &mut ActionsColumn| {
            column.kinds = kinds;
        });
    }

    /// The action `name` of `package`; an error for one the package does not declare.
    pub(crate) fn action_named(view: &View, package: u16, name: &str) -> Checked<ActionId> {
        let found = ActionsColumn::read(view, |column| column.book.named(package, name));
        Ok(found.ok_or_else(|| ApiError::UnknownAbility.fail())?)
    }

    /// The slot kind `name`; an error for one the mode does not declare.
    pub(crate) fn kind_named(view: &View, name: &str) -> Checked<SlotKind> {
        let found = ActionsColumn::read(view, |column| column.kinds.named(name));
        Ok(found.ok_or_else(|| ApiError::UnknownSlotKind.fail())?)
    }

    /// Whether action `id` has charges.
    pub(crate) fn has_charges(view: &View, id: ActionId) -> bool {
        ActionsColumn::read(view, |column| {
            let action = column.book.get(id).expect("an action of the match");
            action.ranks.iter().any(|values| values.charges.is_some())
        })
    }

    /// Whether the unit in row `row` holds action `id` in a slot.
    pub(crate) fn holds(view: &View, row: usize, id: ActionId) -> bool {
        ActionsColumn::read(view, |column| {
            column.run(row).iter().any(|slot| slot.action == Some(id))
        })
    }

    /// How many ranks an action in `kind` has.
    pub(crate) fn kind_ranks(view: &View, kind: SlotKind) -> u8 {
        ActionsColumn::read(view, |column| column.kinds.ranks(kind))
    }

    /// The rank an action in `kind` has as it is granted: 1 for a kind with no `ranks`, 0 for
    /// one whose ranks are learned.
    pub(crate) fn first_rank(view: &View, kind: SlotKind) -> u8 {
        ActionsColumn::read(view, |column| column.kinds.first_rank(kind))
    }

    /// The range of action `id` at `rank`, one of its ranks.
    pub(crate) fn range(view: &View, id: ActionId, rank: u8) -> range::Range {
        ActionsColumn::read(view, |column| {
            column
                .book
                .get(id)
                .expect("an action of the match")
                .values(rank)
                .range
        })
    }

    /// What `read` gives of the column, which every match's pipeline adds.
    fn read<R>(view: &View, read: impl FnOnce(&ActionsColumn) -> R) -> R {
        view.column(read)
            .expect("the action pipeline adds its column to every match's view")
    }

    /// The unit the attacks of the unit in row `row` aim at.
    pub(crate) fn target(view: &View, row: usize) -> Option<StableId> {
        ActionsColumn::read(view, |column| column.rows.now().rows[row].target)
    }

    /// The range of the first weapon of the unit in row `row`, in meters; none with no weapon.
    pub(crate) fn attack_range(view: &View, row: usize) -> Option<Num> {
        ActionsColumn::read(view, |column| column.rows.now().rows[row].attack_range)
    }

    /// How many ability slots the unit in row `row` has.
    pub(crate) fn slot_count(view: &View, row: usize) -> usize {
        ActionsColumn::read(view, |column| column.rows.now().rows[row].slots.len())
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
        self.rows.now().slots(row)
    }
}
