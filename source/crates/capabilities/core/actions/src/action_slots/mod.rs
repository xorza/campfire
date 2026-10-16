use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_declare::PerType;
use campfire_math::{Num, Rounding, Vec3};
use campfire_sim::{Capability, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{ActionId, ActionStart, Rank, Relation, TagSet};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::action::Action;
use crate::action_book::ActionBook;
use crate::action_call::{ActionCall, ResolvedCast, Started};
use crate::action_kind::ActionKind;
use crate::action_range::ActionRange;
use crate::action_slots::slot_times::SlotTimes;
use crate::action_target::ActionTarget;
use crate::channel_call::{ChannelCall, ChannelStep};
use crate::checked_action::CheckedAction;
use crate::in_progress::{InProgress, OrderPhase};
use crate::rank_values::{ChannelRule, ChargeRule, RankValues};
use crate::slot_aim::SlotAim;
use crate::slot_kind::SlotKind;
use crate::slot_kind_book::SlotKindBook;

pub(crate) mod slot_times;
pub(crate) mod timed_slots;

/// A unit's actions: its slots, kind after kind in the mode's order, each an action at a rank,
/// whose times its `SlotTimes` holds; the action it was ordered or has under way, one at a time;
/// and the unit its attacks aim at, which it attacks again each time a weapon is ready, until
/// another order.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionSlots {
    slots: Vec<ActionSlot>,
    underway: Option<InProgress>,
    attack_target: Option<StableId>,
    /// A channel an order, a stop or an interrupt cut, whose `on_interrupt` has yet to run.
    interrupted: Option<ChannelCall>,
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

    /// The order they hold ordered, not yet checked, whose action `book` says is of `kind`.
    pub fn ordered(&self, book: &ActionBook, kind: ActionKind) -> Option<SlotAim> {
        let Some(InProgress::Order {
            aim,
            phase: OrderPhase::Ordered,
        }) = self.in_progress()
        else {
            return None;
        };
        let slot = self.slot(aim.slot)?;
        let of_kind = book.get(slot.action?)?.kind.kind() == kind;
        of_kind.then_some(aim)
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
        Self {
            slots,
            underway: None,
            attack_target: None,
            interrupted: None,
        }
    }

    /// Puts `actions` in `kind` at `rank`, each ready at once in `times`, after the slots of that
    /// kind it has: the slots of later kinds, and an action under way or cut from one, move
    /// along.
    pub fn grant(
        &mut self,
        times: &mut SlotTimes,
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
    /// kinds, and an action under way or cut from one, move along. Where it put them.
    fn insert(
        &mut self,
        kind: SlotKind,
        added: impl ExactSizeIterator<Item = ActionSlot>,
    ) -> usize {
        let at = self.slots.partition_point(|slot| slot.kind <= kind);
        let count = added.len();
        self.slots.splice(at..at, added);
        let moved = |slot: &mut u8| {
            if usize::from(*slot) >= at {
                *slot = u8::try_from(usize::from(*slot) + count).expect("a unit's slots fit u8");
            }
        };
        if let Some(underway) = &mut self.underway {
            moved(underway.slot_mut());
        }
        if let Some(cut) = &mut self.interrupted {
            moved(&mut cut.call.aim.slot);
        }
        at
    }

    /// The first slot of `kind`, where its slots begin, or would.
    pub fn first_of(&self, kind: SlotKind) -> u8 {
        let at = self.slots.partition_point(|slot| slot.kind < kind);
        u8::try_from(at).expect("a unit's slots fit u8")
    }

    /// Puts `action`, or none, in `slot` afresh: ready at once in `times`, with no charges and no
    /// toggle on, as a slot an item fills or leaves; what is under way from the slot stops, a
    /// channel cut.
    pub fn fill(&mut self, times: &mut SlotTimes, slot: u8, action: Option<ActionId>) {
        if self
            .underway
            .is_some_and(|underway| underway.slot() == slot)
        {
            self.stop();
        }
        self.slots[usize::from(slot)].action = action;
        times.reset(slot);
    }

    /// Swaps slots `a` and `b`, each keeping its times in `times`; what is under way or cut from
    /// either follows it.
    pub fn swap(&mut self, times: &mut SlotTimes, a: u8, b: u8) {
        self.slots.swap(usize::from(a), usize::from(b));
        times.swap(a, b);
        let follow = |slot: &mut u8| {
            if *slot == a {
                *slot = b;
            } else if *slot == b {
                *slot = a;
            }
        };
        if let Some(underway) = &mut self.underway {
            follow(underway.slot_mut());
        }
        if let Some(cut) = &mut self.interrupted {
            follow(&mut cut.call.aim.slot);
        }
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

    /// Orders the action in `slot` at `target`, in place of any other action not resolved yet;
    /// an action in `slot` that charges is released instead.
    pub fn order(&mut self, slot: u8, target: ActionTarget) {
        if let Some(InProgress::Charge { aim, released, .. }) = &mut self.underway
            && aim.slot == slot
        {
            *released = true;
            return;
        }
        self.cut_channel();
        self.underway = Some(InProgress::Order {
            aim: SlotAim { slot, target },
            phase: OrderPhase::Ordered,
        });
    }

    pub const fn in_progress(&self) -> Option<InProgress> {
        self.underway
    }

    /// Starts the ordered cast, to resolve in `resolves_at` at the `target` its check kept, as it
    /// `start`ed.
    pub const fn start(&mut self, resolves_at: Tick, target: ActionTarget, start: ActionStart) {
        if let Some(InProgress::Order { aim, phase }) = &mut self.underway {
            aim.target = target;
            *phase = OrderPhase::Started(Started { resolves_at, start });
        }
    }

    /// Makes the ordered cast walk in range of its target, which then owns the unit's walk.
    pub const fn approach(&mut self) {
        if let Some(InProgress::Order { phase, .. }) = &mut self.underway {
            *phase = OrderPhase::Approaching;
        }
    }

    /// Whether an ordered cast walks in range of its target, and so owns the unit's walk.
    pub const fn approaching(&self) -> bool {
        matches!(
            self.underway,
            Some(InProgress::Order {
                phase: OrderPhase::Approaching,
                ..
            })
        )
    }

    /// Starts the ordered cast that passed its checks as `checked`, from `pos` at `now`: a
    /// charged action charges until its most; any other winds up, to resolve as its windup ends.
    pub const fn begin(&mut self, checked: &CheckedAction<'_>, pos: Vec3, now: Tick) {
        let values = checked.values;
        if let Some(most) = values.charge {
            self.charge(checked.target, pos, now, now.after(most));
        } else {
            let start = ActionStart {
                origin: pos,
                charge: None,
            };
            self.start(now.after(values.windup), checked.target, start);
        }
    }

    /// Starts the ordered charged cast at the `target` its check kept, from `origin`, charging
    /// from `now` until `full`.
    pub const fn charge(&mut self, target: ActionTarget, origin: Vec3, now: Tick, full: Tick) {
        if let Some(InProgress::Order { aim, .. }) = self.underway {
            self.underway = Some(InProgress::Charge {
                aim: SlotAim {
                    slot: aim.slot,
                    target,
                },
                origin,
                since: now,
                full,
                released: false,
            });
        }
    }

    /// Releases a charge its order released, or that is full by `now`: it resolves now, with the
    /// share of its most it charged.
    pub fn release(&mut self, now: Tick) {
        let Some(InProgress::Charge {
            aim,
            origin,
            since,
            full,
            released,
        }) = self.underway
        else {
            return;
        };
        if !released && now < full {
            return;
        }
        let ticks = |to: Tick| {
            let held = to.since(since).expect("a charge ends after it starts");
            i64::try_from(held.get()).expect("a span of ticks within the tick limit fits i64")
        };
        let charge = Num::ONE
            .checked_mul_ratio(ticks(now.min(full)), ticks(full), Rounding::NearestEven)
            .expect("a charge's most is a tick at least, and its share at most 1");
        self.underway = Some(InProgress::Order {
            aim,
            phase: OrderPhase::Started(Started {
                resolves_at: now,
                start: ActionStart {
                    origin,
                    charge: Some(charge),
                },
            }),
        });
    }

    /// Cancels a charge under way, which spends nothing.
    pub const fn cancel_charge(&mut self) {
        if let Some(InProgress::Charge { .. }) = self.underway {
            self.underway = None;
        }
    }

    /// Starts an attack with the weapon in `slot` at the attack target, to resolve in
    /// `resolves_at`.
    pub const fn start_attack(&mut self, slot: u8, resolves_at: Tick) {
        let target = self.attack_target.expect("an attack starts at its target");
        self.underway = Some(InProgress::Attack {
            slot,
            target,
            resolves_at,
        });
    }

    /// Stops what is under way and spends nothing: a cast, started or walking in range, goes back
    /// to its order, which starts it again from its check; an attack starts again from the attack
    /// target when it may.
    pub fn interrupt(&mut self) {
        self.cut_channel();
        match &mut self.underway {
            Some(InProgress::Attack { .. } | InProgress::Charge { .. }) => self.underway = None,
            Some(InProgress::Order { phase, .. }) => *phase = OrderPhase::Ordered,
            Some(InProgress::Channel { .. }) | None => {}
        }
    }

    /// Ends what is under way, resolved or not; a channel is cut.
    pub fn stop(&mut self) {
        self.cut_channel();
        self.underway = None;
    }

    /// Cuts a channel under way, for its `on_interrupt` to run: a new order of the unit does, and
    /// so do a stop and an interrupt.
    pub fn cut_channel(&mut self) {
        if let Some(InProgress::Channel { aim, start, .. }) = self.underway {
            self.underway = None;
            self.interrupted = Some(self.channel_call(ActionCall { aim, start }));
        }
    }

    /// Whether damage of more than 0 to the unit cuts the channel under way: its rule, as `book`
    /// gives it, says so.
    pub fn damage_cuts(&self, book: &ActionBook) -> bool {
        let channel = self.channeling().and_then(|slot| self.slot(slot));
        let rule = channel.and_then(|slot| slot.values_in(book)?.channel);
        rule.is_some_and(|rule| rule.damage_interrupts)
    }

    /// `call` of the channel in its slot, at the action and rank the slot holds.
    fn channel_call(&self, call: ActionCall) -> ChannelCall {
        let slot = self.slots[usize::from(call.aim.slot)];
        ChannelCall {
            call,
            action: slot.action.expect("a channel's slot holds its action"),
            rank: slot.rank.expect("a channel's slot is learned"),
        }
    }

    /// Ends the cast under way, due in `now`, as the server resolves it and a client predicts
    /// it: one that `resolved` channels from the next tick, its slot's times spent by
    /// `SlotTimes::spend_cast`; one that did not only stops.
    pub fn finish_cast(&mut self, now: Tick, resolved: Option<&ResolvedCast>) {
        self.stop();
        let Some(&ResolvedCast { call, values }) = resolved else {
            return;
        };
        if let Some(rule) = values.channel {
            self.channel(call.aim, now.after(Ticks::new(1)), rule, call.start);
        }
    }

    /// Channels the action `aim` names from the tick `from`, `rule` its channel at its rank, as
    /// the action `start`ed.
    const fn channel(&mut self, aim: SlotAim, from: Tick, rule: ChannelRule, start: ActionStart) {
        self.underway = Some(InProgress::Channel {
            aim,
            next: from.after(rule.tick),
            ends: from.after(rule.duration),
            start,
        });
    }

    /// The slot whose channel runs, if one does.
    pub const fn channeling(&self) -> Option<u8> {
        match self.underway {
            Some(InProgress::Channel { aim, .. }) => Some(aim.slot),
            _ => None,
        }
    }

    /// Runs the channel at `now`, `tick` the time between its ticks: one `blocked` from casting is
    /// cut; one due ticks, and the next comes `tick` later; one at its end ends whole. What it
    /// gives back are the hooks to run: a cut channel's, since the last step, and a tick's.
    pub fn step_channel(&mut self, now: Tick, blocked: bool, tick: Ticks) -> ChannelStep {
        if blocked {
            self.cut_channel();
        }
        let interrupted = self.interrupted.take();
        let Some(InProgress::Channel {
            aim,
            next,
            ends,
            start,
        }) = self.underway
        else {
            return ChannelStep {
                interrupted,
                ticked: None,
            };
        };
        let ticked = (next <= now).then(|| self.channel_call(ActionCall { aim, start }));
        if ends <= now {
            self.underway = None;
        } else if ticked.is_some() {
            self.underway = Some(InProgress::Channel {
                aim,
                next: next.after(tick),
                ends,
                start,
            });
        }
        ChannelStep {
            interrupted,
            ticked,
        }
    }

    /// Whether a channel runs, or one cut waits for its `on_interrupt`.
    pub const fn channel_due(&self) -> bool {
        self.interrupted.is_some() || matches!(self.underway, Some(InProgress::Channel { .. }))
    }

    /// The unit its attacks aim at.
    pub const fn attack_target(&self) -> Option<StableId> {
        self.attack_target
    }

    /// The target of the attack in its windup, if one is.
    pub const fn attacking(&self) -> Option<StableId> {
        match self.underway {
            Some(InProgress::Attack { target, .. }) => Some(target),
            _ => None,
        }
    }

    /// Attacks `target` from now on; another target than the current one cancels an attack in
    /// its windup and spends nothing.
    pub fn set_attack_target(&mut self, target: Option<StableId>) {
        if target != self.attack_target && self.attacking().is_some() {
            self.underway = None;
        }
        self.attack_target = target;
    }
}

/// A snapshot is untrusted, so slots past the most a unit holds, or not kind after kind, which a
/// grant's search for its kind's end relies on, fail to decode.
impl<'de> Deserialize<'de> for ActionSlots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            slots: Vec<ActionSlot>,
            underway: Option<InProgress>,
            attack_target: Option<StableId>,
            interrupted: Option<ChannelCall>,
        }
        let Fields {
            slots,
            underway,
            attack_target,
            interrupted,
        } = Fields::deserialize(deserializer)?;
        if slots.len() > Self::LIMIT || !slots.is_sorted_by_key(|slot| slot.kind) {
            return Err(D::Error::custom(
                "a unit's slots are at most 256, kind after kind",
            ));
        }
        Ok(Self {
            slots,
            underway,
            attack_target,
            interrupted,
        })
    }
}

impl SimComponent for ActionSlots {
    const NAME: &'static str = "actions.slots";
    type Host = UnitEntity;

    // An action the book lacks, a rank past its ranks, or what is under way of a slot it does
    // not have, an attack of no weapon or an order of one, has no rules for a cast to follow; a
    // slot of a kind the mode lacks, or of an action with ranks its kind has no level for, would
    // be read past the mode's kinds. A place past the world's bound would overflow the distance
    // to another.
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
        // An attack under way is at one of its action's ranks, and started its windup before it
        // resolves, no sooner than tick 0.
        let underway = self.underway.is_none_or(|underway| {
            let slot = self.slots.get(usize::from(underway.slot()));
            let action = slot.and_then(|slot| Some((slot.rank?, book.get(slot.action?)?)));
            let attacks = matches!(underway, InProgress::Attack { .. });
            match underway {
                InProgress::Channel { .. } => return action.is_some(),
                InProgress::Charge { since, full, .. } => {
                    let charges = action.is_some_and(|(rank, action)| {
                        action.has_rank(rank) && action.values(rank).charge.is_some()
                    });
                    return charges && since < full;
                }
                _ => {}
            }
            action.is_some_and(|(rank, action)| {
                let started = || {
                    let windup = action.values(rank).windup;
                    underway
                        .resolves_at()
                        .is_some_and(|at| at.get() >= windup.get())
                };
                let attack = !attacks || (action.has_rank(rank) && started());
                (action.kind.kind() == ActionKind::Attack) == attacks && attack
            })
        });
        // A channel only of an action with one, at a rank it has, and a cut one as well, of a slot
        // it has.
        let channel = match self.underway {
            Some(InProgress::Channel { aim, .. }) => {
                let rule = self
                    .slots
                    .get(usize::from(aim.slot))
                    .and_then(|slot| slot.values_in(book)?.channel);
                rule.is_some()
            }
            _ => true,
        };
        let interrupted = self.interrupted.is_none_or(|cut| {
            let rule = book
                .get(cut.action)
                .and_then(|action| action.values_at(cut.rank)?.channel);
            usize::from(cut.call.aim.slot) < self.slots.len() && rule.is_some()
        });
        let places = self.underway.is_none_or(InProgress::check)
            && self.interrupted.is_none_or(|cut| cut.call.check());
        // A start holds a charge exactly when its action charges at its rank, as `ctx.charge`
        // reads one.
        let charged = |values: Option<&RankValues>, start: ActionStart| {
            values.is_some_and(|values| values.charge.is_some() == start.charge.is_some())
        };
        let slot_values = |slot: u8| {
            let slot = self.slots.get(usize::from(slot))?;
            slot.values_in(book)
        };
        let started = match self.underway {
            Some(InProgress::Order {
                aim,
                phase: OrderPhase::Started(started),
            }) => charged(slot_values(aim.slot), started.start),
            Some(InProgress::Channel { aim, start, .. }) => charged(slot_values(aim.slot), start),
            _ => true,
        };
        let cut_start = self.interrupted.is_none_or(|cut| {
            let values = book
                .get(cut.action)
                .and_then(|action| action.values_at(cut.rank));
            charged(values, cut.call.start)
        });
        self.slots.iter().all(held)
            && underway
            && channel
            && interrupted
            && places
            && started
            && cut_start
    }
}

impl Replication for ActionSlots {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

impl PerType for ActionSlots {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;
}
