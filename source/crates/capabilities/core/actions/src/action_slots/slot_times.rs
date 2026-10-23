use std::iter::{self, Copied};
use std::slice;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::{Num, Rounding, Vec3};
use campfire_script::LivingUnit;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;

use campfire_units::{Body, PoolCost, PoolPayer, PoolPurse, UnitEntity};
use campfire_values::{ActionId, Relation, Team};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::action::Aim;
use crate::action_book::ActionBook;
use crate::action_call::ResolvedCast;
use crate::action_slots::caster::Caster;
use crate::action_slots::{ActionSlot, ActionSlots};
use crate::action_target::ActionTarget;
use crate::checked_action::CheckedAction;
use crate::purse::Purse;
use crate::range_fit::RangeFit;
use crate::rank_toggle::TogglePer;
use crate::rank_values::ChargeRule;
use crate::slot_aim::SlotAim;
use crate::slot_kind::SlotKind;
use crate::targets::Targets;

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

/// What `SlotTimes::start` reads besides the times: the match's book of actions, the tick `now`,
/// the unit as a caster, whose tags and motion may block the action, its slots and what its purse
/// affords, the slot and target `aim` names, how the unit regards a team, `relation`, and the
/// living unit of a stable id, `living`.
#[derive(Debug)]
pub struct ActionCheck<'b, 'a, R, L, P> {
    pub book: &'b ActionBook,
    pub now: Tick,
    pub caster: Caster<'a>,
    pub slots: &'a ActionSlots,
    pub purse: Purse<'a, P>,
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

/// Where a start measures the reach of an action from: the unit's place and body, as `targets`
/// measure reach.
#[derive(Debug, Clone, Copy)]
pub struct StartFrom<'a, 'w, 's> {
    pub position: Vec3,
    pub body: Option<&'a Body>,
    pub targets: &'a Targets<'w, 's>,
}

/// An action its start checked: what passed its checks, and how its target lies against its
/// reach, within it for an action measured from no place.
#[derive(Debug)]
pub struct StartChecked<'b> {
    pub checked: CheckedAction<'b>,
    pub reach: RangeFit,
}

/// What the commit of an action spends of its slot: the slot, the tick `now` it commits at, the
/// first tick it may start again, its action's charge rule at its rank, and, for a toggle it
/// turns on, the tick it pays its cost each second from.
#[derive(Debug, Clone, Copy)]
pub struct SlotCommit {
    pub slot: u8,
    pub now: Tick,
    pub ready_at: Tick,
    pub charges: Option<ChargeRule>,
    pub toggle: Option<Tick>,
}

impl SlotCommit {
    /// What the cast `resolved` commits of its slot as it resolves at `now`: its cooldown and
    /// charge, and its toggle turned on to pay a `second` from now.
    pub fn cast(resolved: &ResolvedCast, now: Tick, second: Ticks) -> Self {
        Self {
            slot: resolved.call.aim.slot,
            now,
            ready_at: now.after(resolved.values.cooldown),
            charges: resolved.values.charges,
            toggle: resolved.values.toggle.map(|_| now.after(second)),
        }
    }
}

impl SlotTimes {
    /// The start step of every kind of action, which each kind's own system takes at its own
    /// moment (04-capabilities/actions.md, Each kind calls the shared steps): `check`'s checks,
    /// and with `from` its point target clamped into its range, as its aim clamps, and its reach
    /// tested; none when it fails a check.
    pub fn start<'b>(
        &self,
        check: ActionCheck<
            'b,
            '_,
            impl Fn(Team) -> Relation,
            impl Fn(StableId) -> Option<LivingUnit>,
            impl PoolPurse,
        >,
        from: Option<StartFrom<'_, '_, '_>>,
    ) -> Option<StartChecked<'b>> {
        let mut checked = self.checked_action(check)?;
        let reach = from.map_or(RangeFit::Within, |from| {
            checked.clamp(from.position, Body::shape_of(from.body), from.targets);
            checked.reach(from.position, from.body, from.targets)
        });
        Some(StartChecked { checked, reach })
    }

    /// The commit step's spend of the slot's time, which each kind takes at its own moment, as its
    /// cost goes through `Payer::pay`: its cooldown to `commit.ready_at`, and under its charge
    /// rule a charge, the timer of the next starting as the slot falls below its most; and the
    /// toggle it turns on.
    pub fn commit(&mut self, commit: SlotCommit) {
        let SlotCommit {
            slot,
            now,
            ready_at,
            charges,
            toggle,
        } = commit;
        let held = &mut self.0[usize::from(slot)];
        held.ready_at = ready_at;
        if let Some(rule) = charges {
            let charges = held
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
        if let Some(next) = toggle {
            held.toggle = Some(next);
        }
    }

    /// The action that `check`'s aim names, of a unit with `check`'s slots and these times, when it
    /// may go on: its caster is not blocked from it, its slot holds a learned action that is ready
    /// and, with charges, holds one, the purse affords its cost in each pool and player resource,
    /// and its target is a living unit the action's filter selects, or the action takes none, which
    /// drops any target the order named.
    fn checked_action<'b>(
        &self,
        check: ActionCheck<
            'b,
            '_,
            impl Fn(Team) -> Relation,
            impl Fn(StableId) -> Option<LivingUnit>,
            impl PoolPurse,
        >,
    ) -> Option<CheckedAction<'b>> {
        let ActionCheck {
            book,
            now,
            caster,
            slots,
            purse,
            aim,
            relation,
            living,
        } = check;
        let (slot, time) = (slots.slot(aim.slot)?, self.slot(aim.slot)?);
        if caster.blocked(Some(slot), book) {
            return None;
        }
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

    pub fn iter(&self) -> Copied<slice::Iter<'_, SlotTime>> {
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

    /// Each slot of `slots` with its time: one time for each slot, as `SlotsMut` keeps them.
    fn paired<'a>(
        &'a self,
        slots: &'a ActionSlots,
    ) -> impl Iterator<Item = (ActionSlot, &'a SlotTime)> + 'a {
        debug_assert_eq!(slots.len(), self.0.len(), "a unit's slots each have a time");
        slots.iter().zip(&self.0)
    }

    /// Each slot of `slots` with its time, to change, as `paired` gives them.
    fn paired_mut<'a>(
        &'a mut self,
        slots: &'a ActionSlots,
    ) -> impl Iterator<Item = (ActionSlot, &'a mut SlotTime)> + 'a {
        debug_assert_eq!(slots.len(), self.0.len(), "a unit's slots each have a time");
        slots.iter().zip(&mut self.0)
    }

    /// Puts `slot` on cooldown until `ready_at`.
    pub fn cool_down(&mut self, slot: u8, ready_at: Tick) {
        self.0[usize::from(slot)].ready_at = ready_at;
    }

    /// Gives each slot of `slots` of `action` that holds charges one more, up to the most `book`
    /// gives the action at the slot's rank, its timer kept.
    pub fn add_charge(&mut self, slots: &ActionSlots, action: ActionId, book: &ActionBook) {
        let rules = book.get(action).expect("an action of the match");
        for (held, time) in self.paired_mut(slots) {
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
        let times = self.paired(slots).map(|(_, time)| time);
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
        self.paired(slots)
            .filter_map(|(slot, time)| {
                let (rule, charges) = (slot.charge_rule(book)?, time.charges?);
                (charges.count < rule.max.get()).then_some(charges.next)
            })
            .min()
    }

    /// Sets the charges of `slot`.
    pub(crate) fn set_charges(&mut self, slot: u8, charges: Option<SlotCharges>) {
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
    /// `pools`, as `book` gives it at the slot's rank, in slot order; one that `pools` cannot pay
    /// with `keep` left for the attack's own cost turns off instead, so the attack still strikes.
    pub fn pay_attack_toggles(
        &mut self,
        slots: &ActionSlots,
        book: &ActionBook,
        pools: &mut impl PoolPayer,
        keep: &PoolCost,
    ) {
        for (held, time) in self.paired_mut(slots) {
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
            let both = toggle.cost.checked_add(keep);
            if both.is_some_and(|both| pools.affords(&both)) {
                pools.pay(&toggle.cost);
            } else {
                time.toggle = None;
            }
        }
    }

    /// Takes `cut` off what is left at `now` of the cooldown of each slot of `slots` that
    /// `select` picks, and of the time to its next charge, to no earlier than `now`.
    pub fn cut(&mut self, slots: &ActionSlots, select: SlotSelect, cut: Cut, now: Tick) {
        for (held, time) in self.paired_mut(slots) {
            if !select.picks(held) {
                continue;
            }
            time.ready_at = cut.of(time.ready_at, now);
            if let Some(charges) = &mut time.charges {
                charges.next = cut.of(charges.next, now);
            }
        }
    }
}

impl<'a> IntoIterator for &'a SlotTimes {
    type Item = SlotTime;
    type IntoIter = Copied<slice::Iter<'a, SlotTime>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The slots a cut of cooldowns picks: those of one action, or those of one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSelect {
    Action(ActionId),
    Kind(SlotKind),
}

impl SlotSelect {
    fn picks(self, slot: ActionSlot) -> bool {
        match self {
            Self::Action(action) => slot.action == Some(action),
            Self::Kind(kind) => slot.kind == kind,
        }
    }
}

/// How much a cut takes off a time: a span, or a share from 0 to 1 of what is left, what stays
/// rounding up to a whole tick, as every time does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    By(Ticks),
    Share(Num),
}

impl Cut {
    fn of(self, at: Tick, now: Tick) -> Tick {
        let left = at.since(now).unwrap_or(Ticks::ZERO).get();
        let kept = match self {
            Self::By(cut) => left.saturating_sub(cut.get()),
            Self::Share(share) => {
                debug_assert!((Num::ZERO..=Num::ONE).contains(&share));
                (Num::ONE - share)
                    .scale_count(left, Rounding::Ceiling)
                    .expect("a share of 0 to 1 of a time is a time")
            }
        };
        now.after(Ticks::new(kept))
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::action_slots::slot_times::{SlotCharges, SlotTimes};

    impl SlotTimes {
        /// Sets the charges of `slot`, whatever its action's rule, as a restore may read them.
        pub fn force_charges(&mut self, slot: u8, charges: Option<SlotCharges>) {
            self.set_charges(slot, charges);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use campfire_values::Rank;

    use super::*;

    #[test]
    #[should_panic(expected = "a unit's slots each have a time")]
    fn times_of_other_slots_than_the_units_fail_where_a_zip_would_drop_some() {
        // Times of one slot, read against two: a zip would stop after the first, as if the second
        // slot held no action, where the pairing panics.
        let kind = SlotKind::new(0);
        let one = ActionSlots::new([(ActionId::new(0), kind, Some(Rank::FIRST))]);
        let mut times = SlotTimes::ready(&one);
        let two = one.with_empty(kind, 1);
        times.cut(
            &two,
            SlotSelect::Kind(kind),
            Cut::Share(Num::QUARTER),
            Tick::ZERO,
        );
    }

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
        times.cut(
            &slots,
            SlotSelect::Kind(kind),
            Cut::Share(Num::QUARTER),
            Tick::ZERO,
        );
        let time = times.slot(0).unwrap();
        assert_eq!(time.ready_at, Tick::new(3 << 60));
        assert_eq!(time.charges.map(|charges| charges.next), Some(Tick::new(3)));
        // A whole cut leaves none; a cut of another kind's slots none of these.
        let other = SlotSelect::Kind(SlotKind::new(1));
        times.cut(&slots, other, Cut::Share(Num::ONE), Tick::ZERO);
        assert_eq!(times.slot(0).unwrap().ready_at, Tick::new(3 << 60));
        let all = SlotSelect::Kind(kind);
        times.cut(&slots, all, Cut::Share(Num::ONE), Tick::ZERO);
        assert_eq!(times.slot(0).unwrap().ready_at, Tick::ZERO);
        // At tick 10, a cooldown to tick 50 has 40 left: 15 off its action keeps 25, to tick 35,
        // where 15 off another action keeps all, and 100 off keeps none, to tick 10, not before.
        times.cool_down(0, Tick::new(50));
        let now = Tick::new(10);
        let of = |action| SlotSelect::Action(ActionId::new(action));
        times.cut(&slots, of(0), Cut::By(Ticks::new(15)), now);
        assert_eq!(times.slot(0).unwrap().ready_at, Tick::new(35));
        times.cut(&slots, of(1), Cut::By(Ticks::new(15)), now);
        assert_eq!(times.slot(0).unwrap().ready_at, Tick::new(35));
        times.cut(&slots, of(0), Cut::By(Ticks::new(100)), now);
        assert_eq!(times.slot(0).unwrap().ready_at, now);
    }

    #[test]
    fn a_commit_spends_its_cooldown_a_charge_and_turns_its_toggle_on() {
        let slots = ActionSlots::new([(ActionId::new(0), SlotKind::new(0), Some(Rank::FIRST))]);
        let mut times = SlotTimes::ready(&slots);
        times.set_charges(
            0,
            Some(SlotCharges {
                count: 3,
                next: Tick::ZERO,
            }),
        );
        let rule = ChargeRule {
            max: NonZeroU8::new(3).unwrap(),
            recharge: Ticks::new(10),
        };
        let commit = |now: u64, toggle: Option<u64>| SlotCommit {
            slot: 0,
            now: Tick::new(now),
            ready_at: Tick::new(now + 4),
            charges: Some(rule),
            toggle: toggle.map(Tick::new),
        };
        // At its most, the first charge spent starts the timer: the next comes 10 ticks from 5.
        times.commit(commit(5, Some(35)));
        let time = times.slot(0).unwrap();
        assert_eq!(time.ready_at, Tick::new(9));
        assert_eq!(
            time.charges,
            Some(SlotCharges {
                count: 2,
                next: Tick::new(15),
            })
        );
        assert_eq!(time.toggle, Some(Tick::new(35)));
        // Below its most, a second keeps the timer, and a commit with no toggle leaves it on.
        times.commit(commit(7, None));
        let time = times.slot(0).unwrap();
        assert_eq!(time.ready_at, Tick::new(11));
        assert_eq!(
            time.charges,
            Some(SlotCharges {
                count: 1,
                next: Tick::new(15),
            })
        );
        assert_eq!(time.toggle, Some(Tick::new(35)));
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
