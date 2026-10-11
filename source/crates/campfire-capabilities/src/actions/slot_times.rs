use std::iter;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::action_call::ResolvedCast;
use crate::actions::action_slots::ActionSlots;
use crate::actions::rank_fields::TogglePer;
use crate::actions::rank_values::ChargeRule;
use crate::actions::slot_kind::SlotKind;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use crate::stats::unit_pools::PoolMetersMut;
use crate::units::action_id::ActionId;
use crate::units::unit_entity::UnitEntity;

/// The times of a unit's action slots, one for each slot of its `ActionSlots`, in their order:
/// what only its owner receives of them, where every client that sees the unit receives the
/// slots.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SlotTimes(Vec<SlotTime>);

/// A slot's times: the first tick it may start again, its charges, for an action with charges
/// once it has a rank, and while its toggle is on, the tick it pays its cost each second next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotTime {
    pub ready_at: Tick,
    pub charges: Option<SlotCharges>,
    pub toggle: Option<Tick>,
}

/// A slot's charges: how many it holds, and the tick the next comes back while it holds fewer
/// than its action's most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotCharges {
    pub count: u8,
    pub next: Tick,
}

/// A unit's slots with their times, each ready at once, as a unit gains slots.
#[derive(Bundle, Debug)]
pub struct TimedSlots {
    times: SlotTimes,
    slots: ActionSlots,
}

impl SlotTime {
    /// Ready at once, with no charges and no toggle on.
    const READY: SlotTime = SlotTime {
        ready_at: Tick::ZERO,
        charges: None,
        toggle: None,
    };

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

impl TimedSlots {
    pub(crate) fn new(slots: ActionSlots) -> TimedSlots {
        TimedSlots {
            times: SlotTimes::ready(&slots),
            slots,
        }
    }
}

impl SlotTimes {
    /// The times of `slots`, each ready at once, with no charges and no toggle on.
    pub(crate) fn ready(slots: &ActionSlots) -> SlotTimes {
        SlotTimes(vec![SlotTime::READY; slots.len()])
    }

    pub fn slot(&self, slot: u8) -> Option<SlotTime> {
        self.0.get(usize::from(slot)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = SlotTime> + '_ {
        self.0.iter().copied()
    }

    /// Puts `count` slots at `at`, each ready at once, as the slots gain them there.
    pub(crate) fn insert(&mut self, at: usize, count: usize) {
        self.0
            .splice(at..at, iter::repeat_n(SlotTime::READY, count));
    }

    /// Makes `slot` ready at once, with no charges and no toggle on, as a slot an item fills or
    /// leaves.
    pub(crate) fn reset(&mut self, slot: u8) {
        self.0[usize::from(slot)] = SlotTime::READY;
    }

    /// Swaps the times of slots `a` and `b`, as the slots swap.
    pub(crate) fn swap(&mut self, a: u8, b: u8) {
        self.0.swap(usize::from(a), usize::from(b));
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub(crate) fn cool_down(&mut self, slot: u8, ready_at: Tick) {
        self.0[usize::from(slot)].ready_at = ready_at;
    }

    /// Spends what starting the action in `slot` at `now` costs of its slot: its cooldown, and
    /// under `rule`, its action's charges at its rank, a charge, the timer of the next starting
    /// as the slot falls below its most.
    pub(crate) fn spend(&mut self, slot: u8, now: Tick, cooldown: Ticks, rule: Option<ChargeRule>) {
        let slot = &mut self.0[usize::from(slot)];
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

    /// Spends the slot of the cast that `resolved` at `now`, as `ActionSlots::finish_cast` ends
    /// it: its cooldown and charge, and its toggle turned on to pay a `second` from now.
    pub(crate) fn spend_cast(&mut self, resolved: &ResolvedCast, now: Tick, second: Ticks) {
        let ResolvedCast { call, values } = *resolved;
        let slot = call.aim.slot;
        self.spend(slot, now, values.cooldown, values.charges);
        if values.toggle.is_some() {
            self.toggle_on(slot, now.after(second));
        }
    }

    /// Gives each slot of `slots` of `action` that holds charges one more, up to the most `book`
    /// gives the action at the slot's rank, its timer kept.
    pub(crate) fn add_charge(&mut self, slots: &ActionSlots, action: ActionId, book: &ActionBook) {
        let rules = book.get(action).expect("an action of the match");
        for (held, time) in slots.iter().zip(&mut self.0) {
            if held.action != Some(action) {
                continue;
            }
            let rule = held.rank.and_then(|rank| rules.values_at(rank)?.charges);
            if let (Some(charges), Some(rule)) = (&mut time.charges, rule) {
                charges.count = (charges.count + 1).min(rule.max.get());
            }
        }
    }

    /// The charges each slot of `slots` holds at `now`, as `book` gives its action's rule at its
    /// rank, for each slot whose charges differ from what it holds: a slot learned since, or a
    /// charge that came back.
    pub(crate) fn charges_due<'a>(
        &'a self,
        slots: &'a ActionSlots,
        book: &'a ActionBook,
        now: Tick,
    ) -> impl Iterator<Item = (u8, Option<SlotCharges>)> + 'a {
        let times = self.0.iter();
        slots
            .indexed()
            .zip(times)
            .filter_map(move |((at, slot), time)| {
                let rule = slot.action.and_then(|action| {
                    let action = book.get(action).expect("a slot's action is in the book");
                    action.values_at(slot.rank?)?.charges
                });
                let charges = time.charges_at(rule, now);
                (charges != time.charges).then_some((at, charges))
            })
    }

    /// Sets the charges of `slot`.
    pub(crate) fn set_charges(&mut self, slot: u8, charges: Option<SlotCharges>) {
        self.0[usize::from(slot)].charges = charges;
    }

    /// Turns the toggle of `slot` on, to pay its cost each second from `next`.
    pub(crate) fn toggle_on(&mut self, slot: u8, next: Tick) {
        self.0[usize::from(slot)].toggle = Some(next);
    }

    /// Turns the toggle of `slot` off.
    pub(crate) fn toggle_off(&mut self, slot: u8) {
        self.0[usize::from(slot)].toggle = None;
    }

    /// Pays the cost of each toggle of `slots` that is on and pays as each attack goes off, from
    /// `pools`, as `book` gives it at the slot's rank; one that `pools` cannot pay turns off
    /// instead.
    pub(crate) fn pay_attack_toggles(
        &mut self,
        slots: &ActionSlots,
        book: &ActionBook,
        pools: &mut PoolMetersMut<'_>,
    ) {
        for (held, time) in slots.iter().zip(&mut self.0) {
            if time.toggle.is_none() {
                continue;
            }
            let toggle = held
                .values_in(book)
                .and_then(|values| values.toggle)
                .expect("a toggle that is on has its rule");
            if toggle.per != TogglePer::Attack {
                continue;
            }
            if pools.read().affords(&toggle.cost) {
                pools.pay(&toggle.cost);
            } else {
                time.toggle = None;
            }
        }
    }

    /// Takes `cut` off the cooldown of each slot of `slots` of `action`, and off the time to its
    /// next charge, to no earlier than `now`.
    pub(crate) fn cut_cooldown(
        &mut self,
        slots: &ActionSlots,
        action: ActionId,
        cut: Ticks,
        now: Tick,
    ) {
        let cut = |at: Tick| {
            let left = at.since(now).unwrap_or(Ticks::ZERO);
            now.after(Ticks::new(left.get().saturating_sub(cut.get())))
        };
        for (held, time) in slots.iter().zip(&mut self.0) {
            if held.action != Some(action) {
                continue;
            }
            time.ready_at = cut(time.ready_at);
            if let Some(charges) = &mut time.charges {
                charges.next = cut(charges.next);
            }
        }
    }

    /// Takes `fraction`, from 0 to 1, of what is left of the cooldown of each slot of `slots` of
    /// `kind` at `now`, and of the time to its next charge; what stays rounds up to a whole tick,
    /// as every time does.
    pub(crate) fn cut_cooldowns(
        &mut self,
        slots: &ActionSlots,
        kind: SlotKind,
        fraction: Num,
        now: Tick,
    ) {
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
        for (held, time) in slots.iter().zip(&mut self.0) {
            if held.kind != kind {
                continue;
            }
            time.ready_at = cut(time.ready_at);
            if let Some(charges) = &mut time.charges {
                charges.next = cut(charges.next);
            }
        }
    }
}

/// A snapshot is untrusted, so times past the most slots a unit holds fail to decode.
impl<'de> Deserialize<'de> for SlotTimes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<SlotTimes, D::Error> {
        let times = Vec::<SlotTime>::deserialize(deserializer)?;
        if times.len() > ActionSlots::LIMIT {
            return Err(D::Error::custom("a unit's slots are at most 256"));
        }
        Ok(SlotTimes(times))
    }
}

impl SimComponent for SlotTimes {
    const NAME: &'static str = "actions.slot_times";
    type Host = UnitEntity;

    // Times of no slot, or slots with no times, would be read past one or the other. A toggle on
    // of an action with none, or charges past the action's most or of one with none, have no
    // rule to follow.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(slots) = world.get::<ActionSlots>(entity) else {
            return false;
        };
        if slots.len() != self.0.len() {
            return false;
        }
        let Some(book) = world.get_resource::<ActionBook>() else {
            return self.0.is_empty();
        };
        slots.iter().zip(&self.0).all(|(held, time)| {
            let values = held.values_in(book);
            let toggle =
                time.toggle.is_none() || values.is_some_and(|values| values.toggle.is_some());
            let charges = time.charges.is_none_or(|charges| {
                let rule = values.and_then(|values| values.charges);
                rule.is_some_and(|rule| charges.count <= rule.max.get())
            });
            toggle && charges
        })
    }
}

impl Replication for SlotTimes {
    const KIND: DataKind = DataKind::Cooldowns;
    type Sending = SentPredicted;
}
