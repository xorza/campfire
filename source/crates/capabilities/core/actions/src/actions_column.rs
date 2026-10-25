use std::ops::Range;
use std::sync::Arc;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::{
    AnyKeptRows, ColumnRows, KeptRows, RowFill, RunMove, UnitRow, View, ViewColumn,
};
use campfire_sim::StableId;
use campfire_values::{ActionId, Rank, TagSet, Team, UnitTags};

use crate::action_book::ActionBook;
use crate::action_range::ActionRange;
use crate::action_slots::ActionSlots;
use crate::action_slots::action_progress::ActionProgress;
use crate::delivery::Delivery;
use crate::slot_kind::SlotKind;
use crate::slot_kind_book::SlotKindBook;
use crate::slot_kinds::SlotKinds;

/// The part of a unit the actions read into its row: its slots.
pub(super) type RowReads = (
    Option<&'static ActionSlots>,
    Option<&'static ActionProgress>,
    Option<&'static UnitTags>,
);

/// What the action pipeline adds to the script view: the match's actions and slot kinds, and each
/// unit's ability slots, the unit its attacks aim at, and the range of its first weapon, a row
/// each.
#[derive(Debug, Default)]
pub struct ActionsColumn {
    book: ActionBook,
    kinds: Arc<SlotKinds>,
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
pub struct SlotRow {
    pub action: Option<ActionId>,
    pub rank: Option<Rank>,
    pub ranks: u8,
}

impl ViewColumn for ActionsColumn {
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, world: &World) -> bool {
        let kinds = &world.resource::<SlotKindBook>().0;
        if !Arc::ptr_eq(kinds, &self.kinds) {
            self.kinds = Arc::clone(kinds);
        }
        // A row holds what its unit's actions give by the book, so another book fills every row
        // again; the kinds fill no row.
        let book = world.resource::<ActionBook>();
        if book.same(&self.book) {
            return false;
        }
        self.book = book.clone();
        true
    }

    /// None: it reads no team's sight.
    fn hides(&self, _: usize, _: Team) -> bool {
        false
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
    /// Adds the row of a unit with `slots`, or with none, of `tags`: each slot's rank and its
    /// action's ranks, the attack target, and the range of its first weapon whose `when` the
    /// unit's tags pass.
    pub fn push(
        &mut self,
        slots: Option<&ActionSlots>,
        progress: Option<&ActionProgress>,
        tags: TagSet,
    ) {
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
        let attack_range = slots.weapon_in(book, tags, None).map(|slot| {
            slots
                .meters_in(book, slot)
                .expect("the load gives every attack a range in meters")
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
            }
        }));
        let end = u32::try_from(rows.slots.len()).expect("slots fit u32");
        rows.rows.push(ActionsRow {
            target: progress.and_then(ActionProgress::attack_target),
            attack_range,
            slots: start..end,
        });
    }

    /// Whether action `id` has charges.
    pub fn has_charges(view: &View, id: ActionId) -> bool {
        Self::read(view, |column| {
            let action = column.book.get(id).expect("an action of the match");
            action.ranks.iter().any(|values| values.charges.is_some())
        })
    }

    /// Whether the unit in row `row` holds action `id` in a slot.
    pub fn holds(view: &View, row: usize, id: ActionId) -> bool {
        Self::read(view, |column| {
            column.run(row).iter().any(|slot| slot.action == Some(id))
        })
    }

    /// How many ranks an action in `kind` has.
    pub fn kind_ranks(view: &View, kind: SlotKind) -> u8 {
        Self::read(view, |column| column.kinds.ranks(kind))
    }

    /// The rank an action in `kind` has as it is granted: the first for a kind with no `ranks`,
    /// none for one whose ranks are learned.
    pub fn first_rank(view: &View, kind: SlotKind) -> Option<Rank> {
        Self::read(view, |column| column.kinds.first_rank(kind))
    }

    /// The range of action `id` at `rank`, one of its ranks.
    pub fn range(view: &View, id: ActionId, rank: Rank) -> ActionRange {
        Self::read(view, |column| {
            column
                .book
                .get(id)
                .expect("an action of the match")
                .values(rank)
                .range
        })
    }

    /// What `read` gives of the column, which every match's pipeline adds.
    fn read<R>(view: &View, read: impl FnOnce(&Self) -> R) -> R {
        view.column(read)
            .expect("the action pipeline adds its column to every match's view")
    }

    /// The unit the attacks of the unit in row `row` aim at.
    pub fn target(view: &View, row: usize) -> Option<StableId> {
        Self::read(view, |column| column.rows.now().rows[row].target)
    }

    /// The range of the first weapon of the unit in row `row`, in meters; none with no weapon.
    pub fn attack_range(view: &View, row: usize) -> Option<Num> {
        Self::read(view, |column| column.rows.now().rows[row].attack_range)
    }

    /// How many ability slots the unit in row `row` has.
    pub fn slot_count(view: &View, row: usize) -> usize {
        Self::read(view, |column| column.rows.now().rows[row].slots.len())
    }

    /// Ability slot `slot` of the unit in row `row`, when it has one.
    pub fn slot(view: &View, row: usize, slot: u8) -> Option<SlotRow> {
        Self::read(view, |column| {
            column.run(row).get(usize::from(slot)).copied()
        })
    }

    /// Whether a learned weapon of `unit`, in row `row`, whose `when` its tags pass, selects
    /// `target`, as `unit` regards it.
    pub fn armed_against(view: &View, row: usize, unit: &UnitRow, target: &UnitRow) -> bool {
        let relation = view.relation(unit.team, target.team);
        let target = Some((relation, target.tags.tags));
        Self::read(view, |column| {
            column.run(row).iter().any(|slot| {
                let action = slot.action.and_then(|action| column.book.get(action));
                action.is_some_and(|action| action.arms(slot.rank, unit.tags.tags, target))
            })
        })
    }

    /// How action `id` delivers, if other than at once.
    pub fn delivers(view: &View, id: ActionId) -> Option<Delivery> {
        Self::read(view, |column| {
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

    /// Adds a unit's actions to the actions' column of the script view.
    pub fn fill_row(
        (slots, progress, tags): ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, Self>,
    ) {
        let tags = tags.map_or(TagSet::NONE, |tags| tags.tags);
        fill.column.push(slots, progress, tags);
    }
}
