use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::{Num, Rounding, Vec3};
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{Dead, UnitEntity};
use campfire_values::ActionStart;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::action_book::ActionBook;
use crate::action_call::{ResolvedCast, StartedOrder};
use crate::action_kind::ActionKind;
use crate::action_slots::ActionSlots;
use crate::action_slots::action_progress::progress_dues::ProgressDues;
use crate::action_target::ActionTarget;
use crate::channel_call::{ChannelCall, ChannelStep};
use crate::checked_action::CheckedAction;
use crate::in_progress::{InProgress, OrderPhase};
use crate::rank_values::RankValues;
use crate::slot_aim::SlotAim;

pub(crate) mod progress_dues;

/// What a unit's actions have under way, beside its slots, whose table it keys on: the action it
/// was ordered or has under way, one at a time; the unit its attacks aim at, which it attacks
/// again each time a weapon is ready, until another order; and a channel an order, a stop or an
/// interrupt cut, whose `on_interrupt` has yet to run. An attack's or a channel's step changes
/// it alone, so what reads the slots' table visits no unit for it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionProgress {
    underway: Option<InProgress>,
    attack_target: Option<StableId>,
    interrupted: Option<ChannelCall>,
}

impl ActionProgress {
    /// The order it holds ordered, not yet checked, whose action in `slots`, as `book` says, is
    /// of `kind`.
    pub fn ordered(
        &self,
        slots: &ActionSlots,
        book: &ActionBook,
        kind: ActionKind,
    ) -> Option<SlotAim> {
        let Some(InProgress::Order {
            aim,
            phase: OrderPhase::Ordered,
        }) = self.in_progress()
        else {
            return None;
        };
        let slot = slots.slot(aim.slot)?;
        let of_kind = book.get(slot.action?)?.kind == kind;
        of_kind.then_some(aim)
    }

    /// Orders the action in `slot` at `target`, in place of any other action not resolved yet;
    /// an action in `slot` that charges is released instead. A channel it cuts makes its unit
    /// due in `dues`.
    pub fn order(&mut self, slot: u8, target: ActionTarget, dues: &mut ProgressDues<'_>) {
        if let Some(InProgress::Charge { aim, released, .. }) = &mut self.underway
            && aim.slot == slot
        {
            *released = true;
            return;
        }
        self.cut();
        self.underway = Some(InProgress::Order {
            aim: SlotAim { slot, target },
            phase: OrderPhase::Ordered,
        });
        dues.push(self);
    }

    pub const fn in_progress(&self) -> Option<InProgress> {
        self.underway
    }

    /// Schedules the ordered cast, to resolve in `resolves_at` at the `target` its check kept, as
    /// it `start`ed: its unit is due then in `dues`.
    pub fn schedule(
        &mut self,
        resolves_at: Tick,
        target: ActionTarget,
        start: ActionStart,
        dues: &mut ProgressDues<'_>,
    ) {
        if let Some(InProgress::Order { aim, phase }) = &mut self.underway {
            aim.target = target;
            *phase = OrderPhase::Started(StartedOrder { resolves_at, start });
        }
        dues.push(self);
    }

    /// Makes the ordered cast walk in range of its target, which then owns the unit's walk.
    pub const fn approach(&mut self) {
        if let Some(InProgress::Order { phase, .. }) = &mut self.underway {
            *phase = OrderPhase::Approaching;
        }
    }

    /// Whether an ordered cast walks in range of its target, and so owns the unit's walk.
    pub const fn is_approaching(&self) -> bool {
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
    pub fn begin(
        &mut self,
        checked: &CheckedAction<'_>,
        position: Vec3,
        now: Tick,
        dues: &mut ProgressDues<'_>,
    ) {
        let values = checked.values;
        if let Some(most) = values.charge {
            self.charge(checked.target, position, now, now.after(most));
        } else {
            let start = ActionStart {
                origin: position,
                charge: None,
            };
            self.schedule(now.after(values.windup), checked.target, start, dues);
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
    /// share of its most it charged, its unit due now in `dues`.
    pub fn release(&mut self, now: Tick, dues: &mut ProgressDues<'_>) {
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
            .checked_mul_div_int(ticks(now.min(full)), ticks(full), Rounding::NearestEven)
            .expect("a charge's most is a tick at least, and its share at most 1");
        self.underway = Some(InProgress::Order {
            aim,
            phase: OrderPhase::Started(StartedOrder {
                resolves_at: now,
                start: ActionStart {
                    origin,
                    charge: Some(charge),
                },
            }),
        });
        dues.push(self);
    }

    /// Cancels a charge under way, which spends nothing.
    pub const fn cancel_charge(&mut self) {
        if let Some(InProgress::Charge { .. }) = self.underway {
            self.underway = None;
        }
    }

    /// Starts an attack with the weapon in `slot` at the attack target, to resolve in
    /// `resolves_at`: its unit is due then in `dues`.
    pub fn schedule_attack(&mut self, slot: u8, resolves_at: Tick, dues: &mut ProgressDues<'_>) {
        let target = self.attack_target.expect("an attack starts at its target");
        self.underway = Some(InProgress::Attack {
            slot,
            target,
            resolves_at,
        });
        dues.push(self);
    }

    /// Stops what is under way and spends nothing: a cast, started or walking in range, goes back
    /// to its order, which starts it again from its check; an attack starts again from the attack
    /// target when it may.
    pub fn interrupt(&mut self, dues: &mut ProgressDues<'_>) {
        self.cut();
        match &mut self.underway {
            Some(InProgress::Attack { .. } | InProgress::Charge { .. }) => self.underway = None,
            Some(InProgress::Order { phase, .. }) => *phase = OrderPhase::Ordered,
            Some(InProgress::Channel { .. }) | None => {}
        }
        dues.push(self);
    }

    /// Ends what is under way, resolved or not; a channel is cut, its unit due in `dues`.
    pub fn end(&mut self, dues: &mut ProgressDues<'_>) {
        self.halt();
        dues.push(self);
    }

    /// Ends what is under way, as `stop` does, but makes no unit due.
    const fn halt(&mut self) {
        self.cut();
        self.underway = None;
    }

    /// Cuts a channel under way, for its `on_interrupt` to run, its unit due in `dues`: a new
    /// order of the unit does, and so do a stop and an interrupt.
    pub fn cut_channel(&mut self, dues: &mut ProgressDues<'_>) {
        self.cut();
        dues.push(self);
    }

    /// Cuts a channel under way, as `cut_channel` does, but makes no unit due.
    const fn cut(&mut self) {
        if let Some(underway @ InProgress::Channel { .. }) = self.underway {
            self.underway = None;
            self.interrupted = underway.channel_call();
        }
    }

    /// Whether damage of more than 0 to the unit cuts the channel under way: its rule, as `book`
    /// gives it at the channel's rank, says so.
    pub fn damage_cuts(&self, book: &ActionBook) -> bool {
        let Some(InProgress::Channel { action, rank, .. }) = self.underway else {
            return false;
        };
        let rule = book
            .get(action)
            .and_then(|action| action.values_at(rank)?.channel);
        rule.is_some_and(|rule| rule.damage_interrupts)
    }

    /// Ends the cast under way, due in `now`, as the server resolves it and a client predicts
    /// it: one that `resolved` channels from the next tick, at the action and rank its slot of
    /// `slots` holds, its slot's times spent by `SlotTimes::spend_cast`; one that did not only
    /// stops.
    pub fn complete_cast(
        &mut self,
        slots: &ActionSlots,
        now: Tick,
        resolved: Option<&ResolvedCast>,
        dues: &mut ProgressDues<'_>,
    ) {
        self.complete(slots, now, resolved);
        dues.push(self);
    }

    /// Ends the cast under way, as `complete_cast` does, but makes no unit due.
    fn complete(&mut self, slots: &ActionSlots, now: Tick, resolved: Option<&ResolvedCast>) {
        self.halt();
        let Some(&ResolvedCast { call, values }) = resolved else {
            return;
        };
        let Some(rule) = values.channel else {
            return;
        };
        let slot = slots.slot(call.aim.slot).expect("a resolved cast's slot");
        let from = now.after(Ticks::new(1));
        self.underway = Some(InProgress::Channel {
            aim: call.aim,
            next: from.after(rule.tick),
            ends: from.after(rule.duration),
            start: call.start,
            action: slot.action.expect("a channel's slot holds its action"),
            rank: slot.rank.expect("a channel's slot is learned"),
        });
    }

    /// The slot whose channel runs, if one does.
    pub const fn channeling(&self) -> Option<u8> {
        match self.underway {
            Some(InProgress::Channel { aim, .. }) => Some(aim.slot),
            _ => None,
        }
    }

    /// Runs the channel at `now`, `tick` the time between its ticks: one due ticks, and the next
    /// comes `tick` later; one at its end ends whole. What it gives back are the hooks to run: a
    /// cut channel's, since the last step, and a tick's. A channel its unit is kept from is cut
    /// first, by `cut_channel`.
    pub fn step_channel(
        &mut self,
        now: Tick,
        tick: Ticks,
        dues: &mut ProgressDues<'_>,
    ) -> ChannelStep {
        let step = self.step(now, tick);
        dues.push(self);
        step
    }

    /// Runs the channel, as `step_channel` does, but makes no unit due.
    fn step(&mut self, now: Tick, tick: Ticks) -> ChannelStep {
        let interrupted = self.interrupted.take();
        let Some(underway @ InProgress::Channel { next, ends, .. }) = self.underway else {
            return ChannelStep {
                interrupted,
                ticked: None,
            };
        };
        let ticked = (next <= now).then(|| underway.channel_call()).flatten();
        if ends <= now {
            self.underway = None;
        } else if let (Some(_), Some(InProgress::Channel { next, .. })) =
            (ticked, self.underway.as_mut())
        {
            *next = next.after(tick);
        }
        ChannelStep {
            interrupted,
            ticked,
        }
    }

    /// Whether a channel runs, or one cut waits for its `on_interrupt`.
    pub const fn is_channel_due(&self) -> bool {
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

    /// Attacks `target` from now on, its unit due in `dues` while it has one; another target than
    /// the current one cancels an attack in its windup and spends nothing.
    pub fn set_attack_target(&mut self, target: Option<StableId>, dues: &mut ProgressDues<'_>) {
        if target != self.attack_target && self.attacking().is_some() {
            self.underway = None;
        }
        self.attack_target = target;
        dues.push(self);
    }

    /// Moves what is under way or cut along, as `count` slots join its unit's at `at`.
    pub(crate) fn slots_added(&mut self, at: usize, count: usize) {
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

    /// Makes what is under way or cut from slot `a` or `b` follow it, as the two swap.
    pub(crate) fn slots_swapped(&mut self, a: u8, b: u8) {
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

    /// Stops what is under way from `slot`, a channel cut, as its slot is filled afresh.
    pub(crate) fn slot_emptied(&mut self, slot: u8, dues: &mut ProgressDues<'_>) {
        if self
            .underway
            .is_some_and(|underway| underway.slot() == slot)
        {
            self.interrupt(dues);
        }
    }
}

impl SimComponent for ActionProgress {
    const NAME: &'static str = "actions.progress";
    type Host = UnitEntity;

    // What is under way of a slot its unit does not have, an attack of no weapon or an order of
    // one, has no rules for a cast to follow; a channel's action and rank are its slot's. A
    // place past the world's bound would overflow the distance to another. A death ends what is
    // under way and the attack target, and no pass of a dead unit's would end them.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<ActionBook>();
        let dead = world.entity(entity).contains::<Dead>();
        let ended = self.underway.is_none() && self.attack_target.is_none();
        (!dead || ended)
            && world
                .get::<ActionSlots>(entity)
                .is_some_and(|slots| self.fits(slots, book))
    }
}

impl ActionProgress {
    /// Whether it holds of a unit with `slots`, its actions in `book`, as a restore checks it.
    fn fits(&self, slots: &ActionSlots, book: &ActionBook) -> bool {
        // An attack under way is at one of its action's ranks, and started its windup before it
        // resolves, no sooner than tick 0.
        let underway = self.underway.is_none_or(|underway| {
            let slot = slots.slot(underway.slot());
            let action = slot.and_then(|slot| Some((slot.rank?, book.get(slot.action?)?)));
            let attacks = matches!(underway, InProgress::Attack { .. });
            match underway {
                InProgress::Channel {
                    action: id,
                    rank: at,
                    ..
                } => {
                    let held =
                        slot.is_some_and(|slot| slot.action == Some(id) && slot.rank == Some(at));
                    let rule = book
                        .get(id)
                        .and_then(|action| action.values_at(at)?.channel);
                    return held && rule.is_some();
                }
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
                (action.kind == ActionKind::Attack) == attacks && attack
            })
        });
        // A cut channel of an action with one, at a rank it has, of a slot it has.
        let interrupted = self.interrupted.is_none_or(|cut| {
            let rule = book
                .get(cut.action)
                .and_then(|action| action.values_at(cut.rank)?.channel);
            usize::from(cut.call.aim.slot) < slots.len() && rule.is_some()
        });
        let places = self.underway.is_none_or(InProgress::check)
            && self.interrupted.is_none_or(|cut| cut.call.check());
        // A start holds a charge exactly when its action charges at its rank, as `ctx.charge`
        // reads one.
        let charged = |values: Option<&RankValues>, start: ActionStart| {
            values.is_some_and(|values| values.charge.is_some() == start.charge.is_some())
        };
        let started = match self.underway {
            Some(InProgress::Order {
                aim,
                phase: OrderPhase::Started(started),
            }) => {
                let values = slots.slot(aim.slot).and_then(|slot| slot.values_in(book));
                charged(values, started.start)
            }
            Some(InProgress::Channel {
                action,
                rank,
                start,
                ..
            }) => charged(
                book.get(action).and_then(|held| held.values_at(rank)),
                start,
            ),
            _ => true,
        };
        let cut_start = self.interrupted.is_none_or(|cut| {
            let values = book
                .get(cut.action)
                .and_then(|action| action.values_at(cut.rank));
            charged(values, cut.call.start)
        });
        underway && interrupted && places && started && cut_start
    }
}

impl Replication for ActionProgress {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::action_book::ActionBook;
    use crate::action_slots::ActionSlots;
    use crate::action_slots::action_progress::ActionProgress;

    impl ActionProgress {
        /// Whether it holds of a unit with `slots`, its actions in `world`'s book, as a restore
        /// checks it.
        pub fn check_against(&self, slots: &ActionSlots, world: &World) -> bool {
            self.fits(slots, world.resource::<ActionBook>())
        }
    }
}
