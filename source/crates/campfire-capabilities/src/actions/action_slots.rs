use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::action_data::TogglePer;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_target::ActionTarget;
use crate::actions::rank_values::{ChannelRule, ChargeRule};
use crate::actions::slot_kind::SlotKind;
use crate::stats::pools::Pools;
use crate::units::action_id::ActionId;

/// A unit's actions: its slots, kind after kind in the mode's order, each an action at a rank
/// with its cooldown; the action it was ordered or has under way, one at a time; and the unit its
/// attacks aim at, which it attacks again each time a weapon is ready, until another order.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlots {
    slots: Vec<ActionSlot>,
    underway: Option<InProgress>,
    attack_target: Option<StableId>,
    /// A channel an order, a stop or an interrupt cut, whose `on_interrupt` has yet to run.
    interrupted: Option<SlotAim>,
}

/// One slot: the action, its kind, its rank, 0 while not learned, the first tick it may start
/// again, and its charges, for an action with charges once it has a rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSlot {
    pub action: ActionId,
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
    /// The action `aim` names, as ordered: not checked yet while `resolves_at` is `None`, then
    /// started, to resolve in that tick. Whether it casts or trains is its action's kind, and only
    /// a cast starts.
    Order {
        aim: SlotAim,
        resolves_at: Option<Tick>,
    },
    /// The channel of the action `aim` names, which resolved at its target: it ticks next in
    /// `next`, and ends in `ends`.
    Channel {
        aim: SlotAim,
        next: Tick,
        ends: Tick,
    },
}

/// What a tick of a channel did: the channel an order, a stop or an interrupt cut since the last
/// one, whose `on_interrupt` runs now, and the slot whose channel ticked, whose
/// `on_channel_tick` runs now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelStep {
    pub(crate) interrupted: Option<SlotAim>,
    pub(crate) ticked: Option<SlotAim>,
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
            | InProgress::Channel {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The aim of a started order that resolves by `now`: of a cast, as only a cast starts.
    pub(crate) fn cast_due(self, now: Tick) -> Option<SlotAim> {
        match self {
            InProgress::Order {
                aim,
                resolves_at: Some(at),
            } if at <= now => Some(aim),
            _ => None,
        }
    }

    /// The tick it resolves in, once started.
    pub(crate) const fn resolves_at(self) -> Option<Tick> {
        match self {
            InProgress::Attack { resolves_at, .. } => Some(resolves_at),
            InProgress::Order { resolves_at, .. } => resolves_at,
            InProgress::Channel { .. } => None,
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
            .map(|(action, kind, rank)| ActionSlot {
                action,
                kind,
                rank,
                ready_at: Tick::ZERO,
                charges: None,
                toggle: None,
            })
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
    /// has: the slots of later kinds, and an action under way from one, move along.
    pub(crate) fn grant(&mut self, kind: SlotKind, actions: &[ActionId], rank: u8) {
        let at = self.slots.partition_point(|slot| slot.kind <= kind);
        let added = actions.iter().map(|&action| ActionSlot {
            action,
            kind,
            rank,
            ready_at: Tick::ZERO,
            charges: None,
            toggle: None,
        });
        self.slots.splice(at..at, added);
        if let Some(underway) = &mut self.underway
            && usize::from(underway.slot()) >= at
        {
            let moved = usize::from(underway.slot()) + actions.len();
            *underway.slot_mut() = u8::try_from(moved).expect("a unit's slots fit u8");
        }
    }

    pub fn slot(&self, slot: u8) -> Option<ActionSlot> {
        self.slots.get(usize::from(slot)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = ActionSlot> + '_ {
        self.slots.iter().copied()
    }

    /// Raises the action in `slot` a rank; the caller checked it has one more.
    pub(crate) fn learn(&mut self, slot: u8) {
        let slot = &mut self.slots[usize::from(slot)];
        slot.rank = slot
            .rank
            .checked_add(1)
            .expect("a rank below the action's ranks");
    }

    /// Orders the action in `slot` at `target`, in place of any other action not resolved yet.
    pub(crate) const fn order(&mut self, slot: u8, target: ActionTarget) {
        self.cut_channel();
        self.underway = Some(InProgress::Order {
            aim: SlotAim { slot, target },
            resolves_at: None,
        });
    }

    pub(crate) const fn in_progress(&self) -> Option<InProgress> {
        self.underway
    }

    /// Starts the ordered cast, to resolve in `resolves_at` at the `target` its check kept.
    pub(crate) const fn start(&mut self, resolves_at: Tick, target: ActionTarget) {
        if let Some(InProgress::Order {
            aim,
            resolves_at: started,
        }) = &mut self.underway
        {
            *started = Some(resolves_at);
            aim.target = target;
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

    /// Stops what is under way and spends nothing: a cast goes back to its order, which starts it
    /// again from its check; an attack starts again from the attack target when it may.
    pub(crate) const fn interrupt(&mut self) {
        self.cut_channel();
        match &mut self.underway {
            Some(InProgress::Attack { .. }) => self.underway = None,
            Some(InProgress::Order { resolves_at, .. }) => *resolves_at = None,
            Some(InProgress::Channel { .. }) | None => {}
        }
    }

    /// Ends what is under way, resolved or not; a channel is cut.
    pub(crate) const fn stop(&mut self) {
        self.cut_channel();
        self.underway = None;
    }

    /// Cuts a channel under way, for its `on_interrupt` to run: a new order of the unit does, and
    /// so do a stop and an interrupt.
    pub(crate) const fn cut_channel(&mut self) {
        if let Some(InProgress::Channel { aim, .. }) = self.underway {
            self.underway = None;
            self.interrupted = Some(aim);
        }
    }

    /// Channels the action `aim` names from the tick `start`, `rule` its channel at its rank.
    pub(crate) const fn channel(&mut self, aim: SlotAim, start: Tick, rule: ChannelRule) {
        self.underway = Some(InProgress::Channel {
            aim,
            next: start.after(rule.tick),
            ends: start.after(rule.duration),
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
    /// cut; one due ticks, and the next comes `tick` later; one at its end ends whole. What it gives
    /// back are the hooks to run: a cut channel's, since the last step, and a tick's.
    pub(crate) fn step_channel(&mut self, now: Tick, blocked: bool, tick: Ticks) -> ChannelStep {
        if blocked {
            self.cut_channel();
        }
        let interrupted = self.interrupted.take();
        let Some(InProgress::Channel { aim, next, ends }) = self.underway else {
            return ChannelStep {
                interrupted,
                ticked: None,
            };
        };
        let ticked = (next <= now).then_some(aim);
        if ends <= now {
            self.underway = None;
        } else if ticked.is_some() {
            self.underway = Some(InProgress::Channel {
                aim,
                next: next.after(tick),
                ends,
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
        for slot in self.slots.iter_mut().filter(|slot| slot.action == action) {
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
        (0..).zip(&self.slots).filter_map(move |(at, slot)| {
            let action = book
                .get(slot.action)
                .expect("a slot's action is in the book");
            let charges = slot.charges_at(action.charge_rule(slot.rank), now);
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
            let toggle = book
                .get(slot.action)
                .and_then(|action| action.toggle_rule(slot.rank))
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
        for slot in self.slots.iter_mut().filter(|slot| slot.action == action) {
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
            book.get(slot.action)
                .is_some_and(|action| action.slots_at(slot.rank))
        };
        // An attack under way is at one of its action's ranks, and started its windup before it
        // resolves, no sooner than tick 0.
        let underway = self.underway.is_none_or(|underway| {
            let slot = self.slots.get(usize::from(underway.slot()));
            let action = slot.and_then(|slot| Some((slot.rank, book.get(slot.action)?)));
            let attacks = matches!(underway, InProgress::Attack { .. });
            if matches!(underway, InProgress::Channel { .. }) {
                return action.is_some();
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
        // A channel only of an action with one, at a rank it has, and a cut one of a slot it has.
        let channel = match self.underway {
            Some(InProgress::Channel { aim, next, ends }) => {
                let rule = self
                    .slots
                    .get(usize::from(aim.slot))
                    .and_then(|slot| book.get(slot.action)?.channel_rule(slot.rank));
                rule.is_some() && next <= Tick::LIMIT && ends <= Tick::LIMIT
            }
            _ => true,
        };
        let interrupted = self
            .interrupted
            .is_none_or(|aim| usize::from(aim.slot) < self.slots.len());
        // A toggle on only for an action with one, at a rank it has.
        let toggles = self.slots.iter().all(|slot| {
            slot.toggle.is_none_or(|next| {
                let toggles = book
                    .get(slot.action)
                    .and_then(|action| action.toggle_rule(slot.rank))
                    .is_some();
                toggles && next <= Tick::LIMIT
            })
        });
        // Charges no more than the action holds at its rank, and only for an action with them.
        let charges = self.slots.iter().all(|slot| {
            slot.charges.is_none_or(|charges| {
                let rule = book
                    .get(slot.action)
                    .and_then(|action| action.charge_rule(slot.rank));
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
