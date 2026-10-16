use std::iter;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::{Num, Rounding};
use campfire_script::LivingUnit;
use campfire_sim::{SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_stats::PoolMetersMut;
use campfire_units::UnitEntity;
use campfire_values::{ActionId, Relation, Team};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::action::Aim;
use crate::action_book::ActionBook;
use crate::action_call::ResolvedCast;
use crate::action_slots::ActionSlots;
use crate::action_target::ActionTarget;
use crate::checked_action::CheckedAction;
use crate::purse::Purse;
use crate::rank_fields::TogglePer;
use crate::rank_values::ChargeRule;
use crate::slot_aim::SlotAim;
use crate::slot_kind::SlotKind;

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

/// What `SlotTimes::checked_action` reads besides the times: the match's book of actions, the
/// tick `now`, the unit's slots and what its purse affords, the slot and target `aim` names, how
/// the unit regards a team, `relation`, and the living unit of a stable id, `living`.
#[derive(Debug)]
pub struct ActionCheck<'b, 'a, R, L> {
    pub book: &'b ActionBook,
    pub now: Tick,
    pub slots: &'a ActionSlots,
    pub purse: Purse<'a>,
    pub aim: SlotAim,
    pub relation: R,
    pub living: L,
}

impl SlotCharges {
    /// One charge more, up to `max`: a slot that holds its most, even 255, holds it still, and
    /// one that holds more, as a rank's rule may give fewer, holds the most.
    fn gain(&mut self, max: u8) {
        self.count = self.count.saturating_add(1).min(max);
    }
}

impl SlotTime {
    /// Ready at once, with no charges and no toggle on.
    const READY: Self = Self {
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

impl SlotTimes {
    /// The action that `check`'s aim names, of a unit with `check`'s slots and these times, when it
    /// may go on: its slot holds a learned action that is ready and, with charges, holds one, the
    /// purse affords its cost in each pool and player resource, and its target is a living unit
    /// the action's filter selects, or the action takes none, which drops any target the order
    /// named.
    pub fn checked_action<'b>(
        &self,
        check: ActionCheck<
            'b,
            '_,
            impl Fn(Team) -> Relation,
            impl Fn(StableId) -> Option<LivingUnit>,
        >,
    ) -> Option<CheckedAction<'b>> {
        let ActionCheck {
            book,
            now,
            slots,
            purse,
            aim,
            relation,
            living,
        } = check;
        let (slot, time) = (slots.slot(aim.slot)?, self.slot(aim.slot)?);
        let (id, rank) = (slot.action?, slot.rank?);
        let action = book.get(id)?;
        let values = *action.values(rank);
        let affords = purse.affords(&values.cost, action.resource_cost(rank));
        let charged =
            values.charges.is_none() || time.charges.is_some_and(|charges| charges.count > 0);
        if now < time.ready_at || !affords || !charged {
            return None;
        }
        let target = match (action.aim, aim.target) {
            (Aim::None, _) => ActionTarget::None,
            (Aim::Point { .. } | Aim::Direction, ActionTarget::Point(at)) => {
                ActionTarget::Point(at)
            }
            (Aim::Unit(filter), ActionTarget::Unit(target))
                if living(target)
                    .is_some_and(|unit| filter.selects(relation(unit.team), unit.tags)) =>
            {
                ActionTarget::Unit(target)
            }
            _ => return None,
        };
        Some(CheckedAction {
            id,
            target,
            action,
            rank,
            values,
        })
    }

    /// The times of `slots`, each ready at once, with no charges and no toggle on.
    pub fn ready(slots: &ActionSlots) -> Self {
        Self(vec![SlotTime::READY; slots.len()])
    }

    pub fn slot(&self, slot: u8) -> Option<SlotTime> {
        self.0.get(usize::from(slot)).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = SlotTime> + '_ {
        self.0.iter().copied()
    }

    /// Puts `count` slots at `at`, each ready at once, as the slots gain them there.
    pub fn insert(&mut self, at: usize, count: usize) {
        self.0
            .splice(at..at, iter::repeat_n(SlotTime::READY, count));
    }

    /// Makes `slot` ready at once, with no charges and no toggle on, as a slot an item fills or
    /// leaves.
    pub fn reset(&mut self, slot: u8) {
        self.0[usize::from(slot)] = SlotTime::READY;
    }

    /// Swaps the times of slots `a` and `b`, as the slots swap.
    pub fn swap(&mut self, a: u8, b: u8) {
        self.0.swap(usize::from(a), usize::from(b));
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub fn cool_down(&mut self, slot: u8, ready_at: Tick) {
        self.0[usize::from(slot)].ready_at = ready_at;
    }

    /// Spends what starting the action in `slot` at `now` costs of its slot: its cooldown, and
    /// under `rule`, its action's charges at its rank, a charge, the timer of the next starting
    /// as the slot falls below its most.
    pub fn spend(&mut self, slot: u8, now: Tick, cooldown: Ticks, rule: Option<ChargeRule>) {
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
    pub fn spend_cast(&mut self, resolved: &ResolvedCast, now: Tick, second: Ticks) {
        let ResolvedCast { call, values } = *resolved;
        let slot = call.aim.slot;
        self.spend(slot, now, values.cooldown, values.charges);
        if values.toggle.is_some() {
            self.toggle_on(slot, now.after(second));
        }
    }

    /// Gives each slot of `slots` of `action` that holds charges one more, up to the most `book`
    /// gives the action at the slot's rank, its timer kept.
    pub fn add_charge(&mut self, slots: &ActionSlots, action: ActionId, book: &ActionBook) {
        let rules = book.get(action).expect("an action of the match");
        for (held, time) in slots.iter().zip(&mut self.0) {
            if held.action != Some(action) {
                continue;
            }
            let rule = held.rank.and_then(|rank| rules.values_at(rank)?.charges);
            if let (Some(charges), Some(rule)) = (&mut time.charges, rule) {
                charges.gain(rule.max.get());
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
                let charges = time.charges_at(slot.charge_rule(book), now);
                (charges != time.charges).then_some((at, charges))
            })
    }

    /// The tick the next charge of a slot of `slots` comes back, as `book` gives its action's
    /// rule at its rank: the earliest of the slots that hold fewer than their most; none when
    /// each holds its most, or holds none.
    pub(crate) fn next_charge(&self, slots: &ActionSlots, book: &ActionBook) -> Option<Tick> {
        slots
            .iter()
            .zip(&self.0)
            .filter_map(|(slot, time)| {
                let (rule, charges) = (slot.charge_rule(book)?, time.charges?);
                (charges.count < rule.max.get()).then_some(charges.next)
            })
            .min()
    }

    /// Sets the charges of `slot`.
    pub fn set_charges(&mut self, slot: u8, charges: Option<SlotCharges>) {
        self.0[usize::from(slot)].charges = charges;
    }

    /// Turns the toggle of `slot` on, to pay its cost each second from `next`.
    pub fn toggle_on(&mut self, slot: u8, next: Tick) {
        self.0[usize::from(slot)].toggle = Some(next);
    }

    /// Turns the toggle of `slot` off.
    pub fn toggle_off(&mut self, slot: u8) {
        self.0[usize::from(slot)].toggle = None;
    }

    /// Pays the cost of each toggle of `slots` that is on and pays as each attack goes off, from
    /// `pools`, as `book` gives it at the slot's rank; one that `pools` cannot pay turns off
    /// instead.
    pub fn pay_attack_toggles(
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
    pub fn cut_cooldown(&mut self, slots: &ActionSlots, action: ActionId, cut: Ticks, now: Tick) {
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
    pub fn cut_cooldowns(&mut self, slots: &ActionSlots, kind: SlotKind, fraction: Num, now: Tick) {
        debug_assert!((Num::ZERO..=Num::ONE).contains(&fraction));
        let keep = Num::ONE - fraction;
        let cut = |at: Tick| {
            let left = at.since(now).unwrap_or(Ticks::ZERO);
            let kept = keep
                .scale_count(left.get(), Rounding::Ceiling)
                .expect("a share of 0 to 1 of a time is a time");
            now.after(Ticks::new(kept))
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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let times = Vec::<SlotTime>::deserialize(deserializer)?;
        if times.len() > ActionSlots::LIMIT {
            return Err(D::Error::custom("a unit's slots are at most 256"));
        }
        Ok(Self(times))
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
        let book = world.resource::<ActionBook>();
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

#[cfg(test)]
mod tests {
    use campfire_values::Rank;

    use super::*;

    #[test]
    fn a_cut_of_any_cooldown_keeps_its_share_rounded_up_to_a_tick() {
        let kind = SlotKind::new(0);
        let slots = ActionSlots::new([(ActionId::new(0), kind, Some(Rank::FIRST))]);
        let mut times = SlotTimes::ready(&slots);
        // From tick 0, a cooldown to the tick limit, 2⁶² ticks, past what a number holds whole,
        // and a charge 3 ticks off. A quarter cut keeps 3 · 2⁶⁰ ticks of the first exactly, and
        // 2.25 of the second, which rounds up to 3.
        times.cool_down(0, Tick::LIMIT);
        let charges = SlotCharges {
            count: 0,
            next: Tick::new(3),
        };
        times.set_charges(0, Some(charges));
        times.cut_cooldowns(&slots, kind, Num::QUARTER, Tick::ZERO);
        let time = times.slot(0).unwrap();
        assert_eq!(time.ready_at, Tick::new(3 << 60));
        assert_eq!(time.charges.map(|charges| charges.next), Some(Tick::new(3)));
        // A whole cut leaves none; a cut of another kind's slots none of these.
        times.cut_cooldowns(&slots, SlotKind::new(1), Num::ONE, Tick::ZERO);
        assert_eq!(times.slot(0).unwrap().ready_at, Tick::new(3 << 60));
        times.cut_cooldowns(&slots, kind, Num::ONE, Tick::ZERO);
        assert_eq!(times.slot(0).unwrap().ready_at, Tick::ZERO);
    }

    #[test]
    fn a_charge_gained_stops_at_the_most_even_at_255() {
        let mut charges = SlotCharges {
            count: 254,
            next: Tick::ZERO,
        };
        charges.gain(255);
        assert_eq!(charges.count, 255);
        charges.gain(255);
        assert_eq!(charges.count, 255);
        // A count past a most that fell comes down to it.
        charges.gain(3);
        assert_eq!(charges.count, 3);
    }
}
