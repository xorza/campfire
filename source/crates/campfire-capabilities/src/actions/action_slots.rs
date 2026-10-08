use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::{Position, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_book::{ActionBook, Checked};
use crate::actions::action_data::TogglePer;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_target::ActionTarget;
use crate::actions::rank_values::{ChannelRule, ChargeRule, RankValues};
use crate::actions::slot_kind::SlotKind;
use crate::stats::pools::Pools;
use crate::units::action_id::ActionId;
use crate::values::action_start::ActionStart;

/// A unit's actions: its slots, kind after kind in the mode's order, each an action at a rank
/// with its cooldown; the action it was ordered or has under way, one at a time; and the unit its
/// attacks aim at, which it attacks again each time a weapon is ready, until another order.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlots {
    slots: Vec<ActionSlot>,
    underway: Option<InProgress>,
    attack_target: Option<StableId>,
    /// A channel an order, a stop or an interrupt cut, whose `on_interrupt` has yet to run.
    interrupted: Option<ChannelCall>,
}

/// One slot: the action, none in an inventory slot whose item has none, its kind, its rank, 0
/// while not learned, the first tick it may start again, and its charges, for an action with
/// charges once it has a rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlot {
    pub action: Option<ActionId>,
    pub kind: SlotKind,
    pub rank: u8,
    pub ready_at: Tick,
    pub charges: Option<SlotCharges>,
    /// While its toggle is on, the tick it pays its cost each second next.
    pub toggle: Option<Tick>,
}

/// A slot's charges: how many it holds, and the tick the next comes back while it holds fewer
/// than its action's most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotCharges {
    pub count: u8,
    pub next: Tick,
}

impl ActionSlot {
    /// A slot of `action`, or none, of `kind` at `rank`, ready at once, with no charges and no
    /// toggle on.
    const fn ready(action: Option<ActionId>, kind: SlotKind, rank: u8) -> ActionSlot {
        ActionSlot {
            action,
            kind,
            rank,
            ready_at: Tick::ZERO,
            charges: None,
            toggle: None,
        }
    }

    /// Its charges at `now` under `rule`, its action's at its rank: none without a rule; full at
    /// first; then each charge that came back by `now`, and no more than the most.
    fn charges_at(&self, rule: Option<ChargeRule>, now: Tick) -> Option<SlotCharges> {
        let rule = rule?;
        let max = rule.max.get();
        let Some(mut charges) = self.charges else {
            return Some(SlotCharges {
                count: max,
                next: now,
            });
        };
        charges.count = charges.count.min(max);
        if rule.recharge == Ticks::ZERO {
            charges.count = max;
        }
        while charges.count < max && charges.next <= now {
            charges.count += 1;
            charges.next = charges.next.after(rule.recharge);
        }
        Some(charges)
    }
}

/// What a unit has under way: an attack in its windup, or an action it was ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum InProgress {
    /// The weapon in `slot`, started at `target` from the attack target, to resolve in
    /// `resolves_at`: an attack is under way only once started.
    Attack {
        slot: u8,
        target: StableId,
        resolves_at: Tick,
    },
    /// The action `aim` names, as ordered, in its `phase`. Whether it casts or trains is its
    /// action's kind, and only a cast walks in range and starts.
    Order { aim: SlotAim, phase: OrderPhase },
    /// The charged action `aim` names, charging from `since` at the target its check kept, from
    /// `origin`: full in `full`, and `released` once its order came again. It resolves when
    /// released, or full.
    Charge {
        aim: SlotAim,
        origin: Position,
        since: Tick,
        full: Tick,
        released: bool,
    },
    /// The channel of the action `aim` names, which resolved at its target, as it `start`ed: it
    /// ticks next in `next`, and ends in `ends`.
    Channel {
        aim: SlotAim,
        next: Tick,
        ends: Tick,
        start: ActionStart,
    },
}

/// How far an ordered action got: not checked yet, walking in range of its target, which then
/// owns its unit's walk, or started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum OrderPhase {
    Ordered,
    Approaching,
    Started(Started),
}

/// A started order: the tick it resolves in, and how it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Started {
    pub(crate) resolves_at: Tick,
    pub(crate) start: ActionStart,
}

/// What a tick of a channel did: the channel an order, a stop or an interrupt cut since the last
/// one, whose `on_interrupt` runs now, and the channel that ticked, whose `on_channel_tick` runs
/// now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelStep {
    pub(crate) interrupted: Option<ChannelCall>,
    pub(crate) ticked: Option<ChannelCall>,
}

/// A call of a channel: the call, and the action and rank its slot held as the channel ran, which
/// a cut channel keeps once an item leaves its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChannelCall {
    pub(crate) call: ActionCall,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
}

/// A call of a started action: the action and its target, and how the action started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActionCall {
    pub(crate) aim: SlotAim,
    pub(crate) start: ActionStart,
}

/// A cast that resolved: its call, at the target its check gave, and its action's values at its
/// rank.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedCast {
    pub(crate) call: ActionCall,
    pub(crate) values: RankValues,
}

/// The action in `slot`, and what it is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SlotAim {
    pub(crate) slot: u8,
    pub(crate) target: ActionTarget,
}

impl InProgress {
    /// The slot whose action is under way.
    pub(crate) const fn slot(self) -> u8 {
        match self {
            InProgress::Attack { slot, .. }
            | InProgress::Order {
                aim: SlotAim { slot, .. },
                ..
            }
            | InProgress::Charge {
                aim: SlotAim { slot, .. },
                ..
            }
            | InProgress::Channel {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The slot, to move when slots are added before it.
    const fn slot_mut(&mut self) -> &mut u8 {
        match self {
            InProgress::Attack { slot, .. }
            | InProgress::Order {
                aim: SlotAim { slot, .. },
                ..
            }
            | InProgress::Charge {
                aim: SlotAim { slot, .. },
                ..
            }
            | InProgress::Channel {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The call of a started order that resolves by `now`: of a cast, as only a cast starts.
    pub(crate) fn cast_due(self, now: Tick) -> Option<ActionCall> {
        match self {
            InProgress::Order {
                aim,
                phase: OrderPhase::Started(Started { resolves_at, start }),
            } if resolves_at <= now => Some(ActionCall { aim, start }),
            _ => None,
        }
    }

    /// The tick it resolves in, once started.
    pub(crate) const fn resolves_at(self) -> Option<Tick> {
        match self {
            InProgress::Attack { resolves_at, .. }
            | InProgress::Order {
                phase: OrderPhase::Started(Started { resolves_at, .. }),
                ..
            } => Some(resolves_at),
            InProgress::Order {
                phase: OrderPhase::Ordered | OrderPhase::Approaching,
                ..
            }
            | InProgress::Charge { .. }
            | InProgress::Channel { .. } => None,
        }
    }
}

impl ActionSlots {
    /// The most slots a unit holds: every index a `u8` holds.
    pub const LIMIT: usize = 256;

    /// Slots of `(action, kind, rank)`, in the order of their kinds, each ready at once.
    pub fn new(slots: impl IntoIterator<Item = (ActionId, SlotKind, u8)>) -> ActionSlots {
        let slots: Vec<ActionSlot> = slots
            .into_iter()
            .map(|(action, kind, rank)| ActionSlot::ready(Some(action), kind, rank))
            .collect();
        debug_assert!(slots.is_sorted_by_key(|slot| slot.kind));
        ActionSlots {
            slots,
            underway: None,
            attack_target: None,
            interrupted: None,
        }
    }

    /// Puts `actions` in `kind` at `rank`, each ready at once, after the slots of that kind it
    /// has: the slots of later kinds, and an action under way or cut from one, move along.
    pub(crate) fn grant(&mut self, kind: SlotKind, actions: &[ActionId], rank: u8) {
        self.insert(
            kind,
            actions
                .iter()
                .map(|&action| ActionSlot::ready(Some(action), kind, rank)),
        );
    }

    /// Puts `count` empty slots of `kind`, an inventory's, of one rank, after the slots of that
    /// kind it has, as `grant` puts actions.
    pub(crate) fn add_empty(&mut self, kind: SlotKind, count: u8) {
        self.insert(kind, (0..count).map(|_| ActionSlot::ready(None, kind, 1)));
    }

    /// Puts `added`, slots of `kind`, after the slots of that kind it has; the slots of later
    /// kinds, and an action under way or cut from one, move along.
    fn insert(&mut self, kind: SlotKind, added: impl ExactSizeIterator<Item = ActionSlot>) {
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
    }

    /// The first slot of `kind`, where its slots begin, or would.
    pub(crate) fn first_of(&self, kind: SlotKind) -> u8 {
        let at = self.slots.partition_point(|slot| slot.kind < kind);
        u8::try_from(at).expect("a unit's slots fit u8")
    }

    /// Puts `action`, or none, in `slot` afresh: ready at once, with no charges and no toggle on,
    /// as a slot an item fills or leaves; what is under way from the slot stops, a channel cut.
    pub(crate) fn fill(&mut self, slot: u8, action: Option<ActionId>) {
        if self
            .underway
            .is_some_and(|underway| underway.slot() == slot)
        {
            self.stop();
        }
        let held = &mut self.slots[usize::from(slot)];
        *held = ActionSlot::ready(action, held.kind, held.rank);
    }

    /// Swaps slots `a` and `b`, each keeping its cooldown, charges and toggle; what is under way
    /// or cut from either follows it.
    pub(crate) fn swap(&mut self, a: u8, b: u8) {
        self.slots.swap(usize::from(a), usize::from(b));
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

    pub(crate) const fn len(&self) -> usize {
        self.slots.len()
    }

    /// Each slot with its index: a range to the last `u8`, as an open one overflows on the
    /// 256th slot a unit may hold.
    pub(crate) fn indexed(&self) -> impl Iterator<Item = (u8, ActionSlot)> + '_ {
        (0..=u8::MAX).zip(self.iter())
    }

    /// Raises the action in `slot` a rank; the caller checked it has one more.
    pub(crate) fn learn(&mut self, slot: u8) {
        let slot = &mut self.slots[usize::from(slot)];
        slot.rank = slot
            .rank
            .checked_add(1)
            .expect("a rank below the action's ranks");
    }

    /// Orders the action in `slot` at `target`, in place of any other action not resolved yet;
    /// an action in `slot` that charges is released instead.
    pub(crate) fn order(&mut self, slot: u8, target: ActionTarget) {
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

    pub(crate) const fn in_progress(&self) -> Option<InProgress> {
        self.underway
    }

    /// Starts the ordered cast, to resolve in `resolves_at` at the `target` its check kept, as it
    /// `start`ed.
    pub(crate) const fn start(
        &mut self,
        resolves_at: Tick,
        target: ActionTarget,
        start: ActionStart,
    ) {
        if let Some(InProgress::Order { aim, phase }) = &mut self.underway {
            aim.target = target;
            *phase = OrderPhase::Started(Started { resolves_at, start });
        }
    }

    /// Makes the ordered cast walk in range of its target, which then owns the unit's walk.
    pub(crate) const fn approach(&mut self) {
        if let Some(InProgress::Order { phase, .. }) = &mut self.underway {
            *phase = OrderPhase::Approaching;
        }
    }

    /// Whether an ordered cast walks in range of its target, and so owns the unit's walk.
    pub(crate) const fn approaching(&self) -> bool {
        matches!(
            self.underway,
            Some(InProgress::Order {
                phase: OrderPhase::Approaching,
                ..
            })
        )
    }

    /// Starts the ordered cast that passed its checks as `checked`, from `position` at `now`: a
    /// charged action charges until its most; any other winds up, to resolve as its windup ends.
    pub(crate) fn begin(&mut self, checked: &Checked<'_>, position: Position, now: Tick) {
        let values = checked.values;
        if let Some(most) = values.charge {
            self.charge(checked.target, position, now, now.after(most));
        } else {
            let start = ActionStart {
                origin: position,
                charge: None,
            };
            self.start(now.after(values.windup), checked.target, start);
        }
    }

    /// Starts the ordered charged cast at the `target` its check kept, from `origin`, charging
    /// from `now` until `full`.
    pub(crate) const fn charge(
        &mut self,
        target: ActionTarget,
        origin: Position,
        now: Tick,
        full: Tick,
    ) {
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
    pub(crate) fn release(&mut self, now: Tick) {
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
            Num::from_int(i64::try_from(held.get()).expect("a charge's ticks fit i64"))
                .expect("a charge's ticks fit a number")
        };
        let charge = ticks(now.min(full))
            .checked_div(ticks(full))
            .expect("a charge's most is a tick at least");
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
    pub(crate) const fn cancel_charge(&mut self) {
        if let Some(InProgress::Charge { .. }) = self.underway {
            self.underway = None;
        }
    }

    /// Starts an attack with the weapon in `slot` at the attack target, to resolve in
    /// `resolves_at`.
    pub(crate) const fn start_attack(&mut self, slot: u8, resolves_at: Tick) {
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
    pub(crate) fn interrupt(&mut self) {
        self.cut_channel();
        match &mut self.underway {
            Some(InProgress::Attack { .. } | InProgress::Charge { .. }) => self.underway = None,
            Some(InProgress::Order { phase, .. }) => *phase = OrderPhase::Ordered,
            Some(InProgress::Channel { .. }) | None => {}
        }
    }

    /// Ends what is under way, resolved or not; a channel is cut.
    pub(crate) fn stop(&mut self) {
        self.cut_channel();
        self.underway = None;
    }

    /// Cuts a channel under way, for its `on_interrupt` to run: a new order of the unit does, and
    /// so do a stop and an interrupt.
    pub(crate) fn cut_channel(&mut self) {
        if let Some(InProgress::Channel { aim, start, .. }) = self.underway {
            self.underway = None;
            self.interrupted = Some(self.channel_call(ActionCall { aim, start }));
        }
    }

    /// `call` of the channel in its slot, at the action and rank the slot holds.
    fn channel_call(&self, call: ActionCall) -> ChannelCall {
        let slot = self.slots[usize::from(call.aim.slot)];
        ChannelCall {
            call,
            action: slot.action.expect("a channel's slot holds its action"),
            rank: slot.rank,
        }
    }

    /// Ends the cast under way, due in `now`, as the server resolves it and a client predicts
    /// it: one that `resolved` spends its slot, turns its toggle on to pay a `second` from now,
    /// and channels from the next tick; one that did not only stops.
    pub(crate) fn finish_cast(
        &mut self,
        now: Tick,
        second: Ticks,
        resolved: Option<&ResolvedCast>,
    ) {
        self.stop();
        let Some(&ResolvedCast { call, values }) = resolved else {
            return;
        };
        let slot = call.aim.slot;
        self.spend(slot, now, values.cooldown, values.charges);
        if values.toggle.is_some() {
            self.toggle_on(slot, now.after(second));
        }
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
    pub(crate) const fn channeling(&self) -> Option<u8> {
        match self.underway {
            Some(InProgress::Channel { aim, .. }) => Some(aim.slot),
            _ => None,
        }
    }

    /// Runs the channel at `now`, `tick` the time between its ticks: one `blocked` from casting is
    /// cut; one due ticks, and the next comes `tick` later; one at its end ends whole. What it
    /// gives back are the hooks to run: a cut channel's, since the last step, and a tick's.
    pub(crate) fn step_channel(&mut self, now: Tick, blocked: bool, tick: Ticks) -> ChannelStep {
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
    pub(crate) const fn channel_due(&self) -> bool {
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
    pub(crate) fn set_attack_target(&mut self, target: Option<StableId>) {
        if target != self.attack_target && self.attacking().is_some() {
            self.underway = None;
        }
        self.attack_target = target;
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub(crate) fn cool_down(&mut self, slot: u8, ready_at: Tick) {
        self.slots[usize::from(slot)].ready_at = ready_at;
    }

    /// Spends what starting the action in `slot` at `now` costs of its slot: its cooldown, and
    /// under `rule`, its action's charges at its rank, a charge, the timer of the next starting
    /// as the slot falls below its most.
    pub(crate) fn spend(&mut self, slot: u8, now: Tick, cooldown: Ticks, rule: Option<ChargeRule>) {
        let slot = &mut self.slots[usize::from(slot)];
        slot.ready_at = now.after(cooldown);
        let Some(rule) = rule else {
            return;
        };
        let charges = slot
            .charges
            .as_mut()
            .expect("an action with charges filled them");
        if charges.count == rule.max.get() {
            charges.next = now.after(rule.recharge);
        }
        charges.count = charges
            .count
            .checked_sub(1)
            .expect("an action starts only with a charge");
    }

    /// Gives each slot of `action` that holds charges one more, up to the most `book` gives the
    /// action at the slot's rank, its timer kept.
    pub(crate) fn add_charge(&mut self, action: ActionId, book: &ActionBook) {
        let rules = book.get(action).expect("an action of the match");
        for slot in self
            .slots
            .iter_mut()
            .filter(|slot| slot.action == Some(action))
        {
            if let (Some(charges), Some(rule)) = (&mut slot.charges, rules.charge_rule(slot.rank)) {
                charges.count = (charges.count + 1).min(rule.max.get());
            }
        }
    }

    /// The charges each slot holds at `now`, as `book` gives its action's rule at its rank, for
    /// each slot whose charges differ from what it holds: a slot learned since, or a charge that
    /// came back.
    pub(crate) fn charges_due<'a>(
        &'a self,
        book: &'a ActionBook,
        now: Tick,
    ) -> impl Iterator<Item = (u8, Option<SlotCharges>)> + 'a {
        self.indexed().filter_map(move |(at, slot)| {
            let rule = slot.action.and_then(|action| {
                let action = book.get(action).expect("a slot's action is in the book");
                action.charge_rule(slot.rank)
            });
            let charges = slot.charges_at(rule, now);
            (charges != slot.charges).then_some((at, charges))
        })
    }

    /// Turns the toggle of `slot` on, to pay its cost each second from `next`.
    pub(crate) fn toggle_on(&mut self, slot: u8, next: Tick) {
        self.slots[usize::from(slot)].toggle = Some(next);
    }

    /// Turns the toggle of `slot` off.
    pub(crate) fn toggle_off(&mut self, slot: u8) {
        self.slots[usize::from(slot)].toggle = None;
    }

    /// Pays the cost of each toggle that is on and pays as each attack goes off, from `pools`, as
    /// `book` gives it at the slot's rank; one that `pools` cannot pay turns off instead.
    pub(crate) fn pay_attack_toggles(&mut self, book: &ActionBook, pools: &mut Pools) {
        for slot in self.slots.iter_mut().filter(|slot| slot.toggle.is_some()) {
            let toggle = slot
                .action
                .and_then(|action| book.get(action)?.toggle_rule(slot.rank))
                .expect("a toggle that is on has its rule");
            if toggle.per != TogglePer::Attack {
                continue;
            }
            if pools.affords(&toggle.cost) {
                pools.pay(&toggle.cost);
            } else {
                slot.toggle = None;
            }
        }
    }

    /// Sets the charges of `slot`.
    pub(crate) fn set_charges(&mut self, slot: u8, charges: Option<SlotCharges>) {
        self.slots[usize::from(slot)].charges = charges;
    }

    /// Takes `cut` off the cooldown of each slot of `action`, and off the time to its next
    /// charge, to no earlier than `now`.
    pub(crate) fn cut_cooldown(&mut self, action: ActionId, cut: Ticks, now: Tick) {
        let cut = |at: Tick| {
            let left = at.since(now).unwrap_or(Ticks::ZERO);
            now.after(Ticks::new(left.get().saturating_sub(cut.get())))
        };
        for slot in self
            .slots
            .iter_mut()
            .filter(|slot| slot.action == Some(action))
        {
            slot.ready_at = cut(slot.ready_at);
            if let Some(charges) = &mut slot.charges {
                charges.next = cut(charges.next);
            }
        }
    }

    /// Takes `fraction`, from 0 to 1, of what is left of the cooldown of each slot of `kind` at
    /// `now`, and of the time to its next charge; what stays rounds up to a whole tick, as every
    /// time does.
    pub(crate) fn cut_cooldowns(&mut self, kind: SlotKind, fraction: Num, now: Tick) {
        debug_assert!((Num::ZERO..=Num::ONE).contains(&fraction));
        let keep = Num::ONE - fraction;
        let cut = |at: Tick| {
            let left = at.since(now).unwrap_or(Ticks::ZERO);
            let kept = Num::from_int(i64::try_from(left.get()).expect("a cooldown fits i64"))
                .and_then(|left| left.checked_mul(keep))
                .expect("a cooldown within the tick limit scales within a number");
            now.after(Ticks::new(
                u64::try_from(kept.ceil()).expect("what is kept is not negative"),
            ))
        };
        for slot in self.slots.iter_mut().filter(|slot| slot.kind == kind) {
            slot.ready_at = cut(slot.ready_at);
            if let Some(charges) = &mut slot.charges {
                charges.next = cut(charges.next);
            }
        }
    }
}

impl SimComponent for ActionSlots {
    const NAME: &'static str = "actions.slots";

    // An action the book lacks, a rank past its ranks, or what is under way of a slot it does
    // not have, an attack of no weapon or an order of one, has no rules for a cast to follow.
    fn check(&self, world: &World, _: Entity) -> bool {
        let Some(book) = world.get_resource::<ActionBook>() else {
            return self.slots.is_empty() && self.underway.is_none();
        };
        let held = |slot: &ActionSlot| {
            slot.action.is_none_or(|action| {
                book.get(action)
                    .is_some_and(|action| action.slots_at(slot.rank))
            })
        };
        // An attack under way is at one of its action's ranks, and started its windup before it
        // resolves, no sooner than tick 0.
        let underway = self.underway.is_none_or(|underway| {
            let slot = self.slots.get(usize::from(underway.slot()));
            let action = slot.and_then(|slot| Some((slot.rank, book.get(slot.action?)?)));
            let attacks = matches!(underway, InProgress::Attack { .. });
            match underway {
                InProgress::Channel { .. } => return action.is_some(),
                InProgress::Charge { since, full, .. } => {
                    let charges = action.is_some_and(|(rank, action)| {
                        action.has_rank(rank) && action.values(rank).charge.is_some()
                    });
                    return charges && since < full && full <= Tick::LIMIT;
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
            Some(InProgress::Channel {
                aim, next, ends, ..
            }) => {
                let rule = self
                    .slots
                    .get(usize::from(aim.slot))
                    .and_then(|slot| book.get(slot.action?)?.channel_rule(slot.rank));
                rule.is_some() && next <= Tick::LIMIT && ends <= Tick::LIMIT
            }
            _ => true,
        };
        let interrupted = self.interrupted.is_none_or(|cut| {
            let rule = book
                .get(cut.action)
                .and_then(|action| action.channel_rule(cut.rank));
            usize::from(cut.call.aim.slot) < self.slots.len() && rule.is_some()
        });
        // A toggle on only for an action with one, at a rank it has.
        let toggles = self.slots.iter().all(|slot| {
            slot.toggle.is_none_or(|next| {
                let toggles = slot
                    .action
                    .and_then(|action| book.get(action)?.toggle_rule(slot.rank))
                    .is_some();
                toggles && next <= Tick::LIMIT
            })
        });
        // Charges no more than the action holds at its rank, and only for an action with them.
        let charges = self.slots.iter().all(|slot| {
            slot.charges.is_none_or(|charges| {
                let rule = slot
                    .action
                    .and_then(|action| book.get(action)?.charge_rule(slot.rank));
                rule.is_some_and(|rule| charges.count <= rule.max.get())
                    && charges.next <= Tick::LIMIT
            })
        });
        let times = self.slots.iter().all(|slot| slot.ready_at <= Tick::LIMIT)
            && self
                .underway
                .and_then(InProgress::resolves_at)
                .is_none_or(|at| at <= Tick::LIMIT);
        self.slots.len() <= ActionSlots::LIMIT
            && self.slots.iter().all(held)
            && underway
            && times
            && charges
            && toggles
            && channel
            && interrupted
    }
}
