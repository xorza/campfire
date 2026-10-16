use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::PerType;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{ActionId, Rank, Relation, TagSet};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::action::Action;
use crate::action_book::ActionBook;
use crate::action_kind::ActionKind;
use crate::action_range::ActionRange;
use crate::action_slots::action_progress::ActionProgress;
use crate::action_slots::slot_times::SlotTimes;
use crate::rank_values::{ChargeRule, RankValues};
use crate::slot_kind::SlotKind;
use crate::slot_kind_book::SlotKindBook;

pub(crate) mod action_progress;
pub(crate) mod slot_times;
pub(crate) mod timed_slots;

/// A unit's actions: its slots, kind after kind in the mode's order, each an action at a rank,
/// whose times its `SlotTimes` holds, and what it has under way its `ActionProgress`.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ActionSlots {
    slots: Vec<ActionSlot>,
}

/// One slot: the action, none in an inventory slot whose item has none, its kind, and its rank,
/// none while not learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlot {
    pub action: Option<ActionId>,
    pub kind: SlotKind,
    pub rank: Option<Rank>,
}

impl ActionSlot {
    /// Its action's values at its rank, as `book` gives them; none for a slot with no action, one
    /// not learned, or at a rank its action lacks.
    pub fn values_in(self, book: &ActionBook) -> Option<&RankValues> {
        book.get(self.action?)?.values_at(self.rank?)
    }

    /// Its action's rule of charges at its rank, as `book` gives it; none for a slot with no
    /// action, one not learned, or an action without charges at its rank.
    pub(crate) fn charge_rule(self, book: &ActionBook) -> Option<ChargeRule> {
        let action = book
            .get(self.action?)
            .expect("a slot's action is in the book");
        action.values_at(self.rank?)?.charges
    }
}

impl ActionSlots {
    /// The slot of the first learned weapon of these slots, of `book`, whose filter selects a unit
    /// of `tags` its unit regards with `relation`, ready or not; or with `None`, the first learned
    /// weapon at all.
    pub fn weapon_in(&self, book: &ActionBook, target: Option<(Relation, TagSet)>) -> Option<u8> {
        let at = self.iter().position(|slot| {
            slot.action.is_some_and(|action| {
                let action = book.get(action).expect("a slot's action is in the book");
                Action::arms(slot.rank, action.weapon_filter(), target)
            })
        })?;
        Some(u8::try_from(at).expect("a unit's slots fit u8"))
    }

    /// The range of the learned action in `slot`, of `book`, at its rank.
    pub fn range_in(&self, book: &ActionBook, slot: u8) -> ActionRange {
        let slot = self.slot(slot).expect("a unit's slot");
        slot.values_in(book)
            .expect("a learned slot's action is in the book")
            .range
    }

    /// The range in meters of the action in `slot`, of `book`, one that reaches in meters at
    /// every rank, as a build's and a gather's do.
    pub fn meters_in(&self, book: &ActionBook, slot: u8) -> Num {
        let ActionRange::Meters(range) = self.range_in(book, slot) else {
            panic!("a build's and a gather's ranks reach in meters, as their book checked");
        };
        range
    }

    /// The kind of the action in `slot`, as `book` gives it; none for a slot that is missing or
    /// holds none.
    pub fn kind_in(&self, book: &ActionBook, slot: u8) -> Option<ActionKind> {
        let action = self.slot(slot)?.action?;
        Some(book.get(action)?.kind.kind())
    }

    /// The most slots a unit holds: every index a `u8` holds.
    pub const LIMIT: usize = 256;

    /// Slots of `(action, kind, rank)`, in the order of their kinds, each at its rank, none before
    /// it is learned.
    pub fn new(slots: impl IntoIterator<Item = (ActionId, SlotKind, Option<Rank>)>) -> Self {
        let slots: Vec<ActionSlot> = slots
            .into_iter()
            .map(|(action, kind, rank)| ActionSlot {
                action: Some(action),
                kind,
                rank,
            })
            .collect();
        assert!(
            slots.is_sorted_by_key(|slot| slot.kind),
            "a unit's slots are kind after kind"
        );
        Self { slots }
    }

    /// Puts `actions` in `kind` at `rank`, each ready at once in `times`, after the slots of that
    /// kind it has: the slots of later kinds, and an action under way or cut from one, move
    /// along.
    pub fn grant(
        &mut self,
        (times, progress): (&mut SlotTimes, &mut ActionProgress),
        kind: SlotKind,
        actions: &[ActionId],
        rank: Option<Rank>,
    ) {
        let added = actions.iter().map(|&action| ActionSlot {
            action: Some(action),
            kind,
            rank,
        });
        let at = self.insert(kind, added);
        times.insert(at, actions.len());
        progress.slots_added(at, actions.len());
    }

    /// The same slots with `count` empty slots of `kind`, an inventory's or a use slot's, of one
    /// rank, after the slots of that kind, as a new unit's are, before it has times.
    #[must_use]
    pub fn with_empty(mut self, kind: SlotKind, count: u8) -> Self {
        let rank = Some(Rank::FIRST);
        let added = (0..count).map(|_| ActionSlot {
            action: None,
            kind,
            rank,
        });
        self.insert(kind, added);
        self
    }

    /// Puts `added`, slots of `kind`, after the slots of that kind it has; the slots of later
    /// kinds move along. Where it put them.
    fn insert(
        &mut self,
        kind: SlotKind,
        added: impl ExactSizeIterator<Item = ActionSlot>,
    ) -> usize {
        let at = self.slots.partition_point(|slot| slot.kind <= kind);
        self.slots.splice(at..at, added);
        at
    }

    /// The first slot of `kind`, where its slots begin, or would.
    pub fn first_of(&self, kind: SlotKind) -> u8 {
        let at = self.slots.partition_point(|slot| slot.kind < kind);
        u8::try_from(at).expect("a unit's slots fit u8")
    }

    /// Puts `action`, or none, in `slot` afresh: ready at once in `times`, with no charges and no
    /// toggle on, as a slot an item fills or leaves; what is under way from the slot stops in
    /// `progress`, a channel cut.
    pub fn fill(
        &mut self,
        (times, progress): (&mut SlotTimes, &mut ActionProgress),
        slot: u8,
        action: Option<ActionId>,
    ) {
        progress.slot_emptied(slot);
        self.slots[usize::from(slot)].action = action;
        times.reset(slot);
    }

    /// Swaps slots `a` and `b`, each keeping its times in `times`; what is under way or cut from
    /// either follows it in `progress`.
    pub fn swap(&mut self, (times, progress): (&mut SlotTimes, &mut ActionProgress), a: u8, b: u8) {
        self.slots.swap(usize::from(a), usize::from(b));
        times.swap(a, b);
        progress.slots_swapped(a, b);
    }

    pub fn slot(&self, slot: u8) -> Option<ActionSlot> {
        self.slots.get(usize::from(slot)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = ActionSlot> + '_ {
        self.slots.iter().copied()
    }

    pub const fn len(&self) -> usize {
        self.slots.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Each slot with its index: a range to the last `u8`, as an open one overflows on the
    /// 256th slot a unit may hold.
    pub fn indexed(&self) -> impl Iterator<Item = (u8, ActionSlot)> + '_ {
        (0..=u8::MAX).zip(self.iter())
    }

    /// Raises the action in `slot` a rank; the caller checked it has one more.
    pub fn learn(&mut self, slot: u8) {
        let slot = &mut self.slots[usize::from(slot)];
        let next = slot.rank.map_or(Some(Rank::FIRST), Rank::next);
        slot.rank = Some(next.expect("a rank below the action's ranks"));
    }
}

/// A snapshot is untrusted, so slots past the most a unit holds, or not kind after kind, which a
/// grant's search for its kind's end relies on, fail to decode.
impl<'de> Deserialize<'de> for ActionSlots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let slots = Vec::<ActionSlot>::deserialize(deserializer)?;
        if slots.len() > Self::LIMIT || !slots.is_sorted_by_key(|slot| slot.kind) {
            return Err(D::Error::custom(
                "a unit's slots are at most 256, kind after kind",
            ));
        }
        Ok(Self { slots })
    }
}

impl SimComponent for ActionSlots {
    const NAME: &'static str = "actions.slots";
    type Host = UnitEntity;

    // An action the book lacks or a rank past its ranks has no rules for a cast to follow; a
    // slot of a kind the mode lacks, or of an action with ranks its kind has no level for, would
    // be read past the mode's kinds.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<ActionBook>();
        let kinds = &world.resource::<SlotKindBook>().0;
        let held = |slot: &ActionSlot| {
            let Some(action) = slot.action else {
                return kinds.holds(slot.kind, 0);
            };
            book.get(action).is_some_and(|action| {
                action.slots_at(slot.rank) && kinds.holds(slot.kind, action.ranks.len())
            })
        };
        self.slots.iter().all(held)
    }
}

impl Replication for ActionSlots {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

impl PerType for ActionSlots {
    const SCRIPTS_ONLY: bool = false;
}
