use bevy_ecs::component::Component;
use campfire_math::Num;
use std::num::NonZeroU32;

use campfire_sim::{SimComponent, StableId, Tick, Ticks};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionId;
use crate::scripts::state_value::StateValue;
use crate::stats::live_param::LiveParam;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_data::Reapply;
use crate::stats::stat_op::StatOp;
use crate::units::tag_set::TagSet;

/// The modifiers a unit carries, by id, then source, one instance of an id from each source.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Modifiers(Vec<Instance>);

/// A modifier a unit carries, its numbers resolved when it was applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Instance {
    pub(crate) id: ModifierId,
    /// The unit that applied it; none from the mode.
    pub(crate) source: Option<StableId>,
    /// The ability whose cast, projectile, area or modifier applied it, or whose passive it is,
    /// at its rank on the source.
    pub(crate) ability: Option<ActionId>,
    pub(crate) rank: u8,
    /// Whether it is an ability's passive, which a death keeps, and whether an aura or its
    /// carrier's player holds it.
    pub(crate) passive: bool,
    pub(crate) held: bool,
    /// The radius of the aura it gives, when its modifier has one.
    pub(crate) aura_radius: Option<Num>,
    pub(crate) stacks: u32,
    /// The first tick it no longer holds; none until removed.
    pub(crate) until: Option<Tick>,
    /// How long each stack holds, when its stacks end one by one.
    pub(crate) stack_life: Option<Ticks>,
    /// With a stack life, when its stacks end: by tick, ascending, their counts adding to its
    /// stacks; empty without one.
    pub(crate) stack_ends: Vec<StackEnd>,
    /// When its `on_interval` comes, when its modifier has an interval.
    pub(crate) interval: Option<Interval>,
    /// What is left of its shield.
    pub(crate) shield: Option<Num>,
    /// What it adds to each stat a stack, by the stat's place in the stat book.
    pub(crate) stats: Vec<StatShare>,
    /// The tags it grants its carrier.
    pub(crate) tags: TagSet,
    /// Its script state, in the order of its fields' names.
    pub(crate) state: Vec<StateValue>,
}

/// A modifier's interval: every `every` ticks, the next in tick `next`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Interval {
    pub(crate) every: Ticks,
    pub(crate) next: Tick,
}

/// The stacks of an instance that end as tick `until` starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StackEnd {
    pub(crate) until: Tick,
    pub(crate) count: u32,
}

/// A modifier's change of one stat a stack: `value`, which the refresh reads again from `live`
/// when the change reads a scaling table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StatShare {
    pub(crate) stat: u16,
    pub(crate) op: StatOp,
    pub(crate) value: Num,
    pub(crate) live: Option<LiveParam>,
}

/// A modifier applied to a unit, its numbers resolved: its instance as it would be new, with
/// how a second application from its source acts and its stack limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Application {
    pub(crate) instance: Instance,
    pub(crate) reapply: Reapply,
    pub(crate) max_stacks: Option<NonZeroU32>,
}

impl Modifiers {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Instance> + Clone {
        self.0.iter()
    }

    /// The instance of `id` from `source`.
    pub(crate) fn get(&self, id: ModifierId, source: Option<StableId>) -> Option<&Instance> {
        let at = self.find(id, source).ok()?;
        Some(&self.0[at])
    }

    pub(crate) fn get_mut(
        &mut self,
        id: ModifierId,
        source: Option<StableId>,
    ) -> Option<&mut Instance> {
        let at = self.find(id, source).ok()?;
        Some(&mut self.0[at])
    }

    /// Applies `application`: a new instance, or one more application of the instance its
    /// source holds, as its `reapply` says. `refresh` takes the new numbers and end and keeps the
    /// stacks; `stack` does too, and adds a stack up to the limit, a stack past it ending the
    /// stack that ends soonest in its place; `ignore` leaves the instance. The script state
    /// stays.
    pub(crate) fn apply(&mut self, application: Application) {
        let Application {
            instance: new,
            reapply,
            max_stacks,
        } = application;
        let at = match self.find(new.id, new.source) {
            Ok(at) => at,
            Err(at) => {
                self.0.insert(at, new);
                return;
            }
        };
        let held = &mut self.0[at];
        match reapply {
            Reapply::Ignore => {}
            Reapply::Refresh => held.renew(new),
            Reapply::Stack => {
                let stacks = reapply.stacks(held.stacks, max_stacks);
                let added = new.stack_ends.first().map(|end| end.until);
                held.renew(new);
                if let Some(until) = added {
                    if stacks == held.stacks {
                        held.end_soonest(1);
                    }
                    held.add_ends(1, until);
                }
                held.stacks = stacks;
            }
        }
    }

    /// Removes the instance of `id` from `source`; whether it held one.
    pub(crate) fn remove(&mut self, id: ModifierId, source: Option<StableId>) -> bool {
        let Ok(at) = self.find(id, source) else {
            return false;
        };
        self.0.remove(at);
        true
    }

    /// Ends what no longer holds as tick `now` starts: each stack whose end it reached, then
    /// each instance whose end it reached, or whose stacks ending one by one all ended, but a
    /// passive, which stays with none; whether any ended.
    pub(crate) fn expire(&mut self, now: Tick) -> bool {
        let ends = |instance: &Instance| {
            instance.until.is_some_and(|until| until <= now)
                || instance
                    .stack_ends
                    .first()
                    .is_some_and(|end| end.until <= now)
        };
        if !self.0.iter().any(ends) {
            return false;
        }
        self.0.retain_mut(|instance| {
            let ended = instance.stack_ends.partition_point(|end| end.until <= now);
            if ended > 0 {
                let count: u32 = instance
                    .stack_ends
                    .drain(..ended)
                    .map(|end| end.count)
                    .sum();
                instance.stacks -= count;
                if instance.stacks == 0 {
                    return instance.passive;
                }
            }
            instance.until.is_none_or(|until| until > now)
        });
        true
    }

    /// Counts the intervals of tick `now`: each instance whose interval comes, and whose tags
    /// `takes_effect` lets act, goes to `due`, by id and source, and its next comes an interval
    /// later; whether any came.
    pub(crate) fn advance_intervals(
        &mut self,
        now: Tick,
        takes_effect: impl Fn(TagSet) -> bool,
        mut due: impl FnMut(ModifierId, Option<StableId>),
    ) -> bool {
        let mut any = false;
        for instance in &mut self.0 {
            if instance.interval_due(now) {
                if takes_effect(instance.tags) {
                    due(instance.id, instance.source);
                }
                any = true;
            }
        }
        any
    }

    /// Spends shields on `amount` of damage: of the instances whose tags `takes_effect` lets act,
    /// the shield that ends soonest first, one with no end last, and shields with the same end in
    /// the order kept; a shield spent to 0 ends its instance. What is left of the amount.
    pub(crate) fn absorb(&mut self, mut amount: Num, takes_effect: impl Fn(TagSet) -> bool) -> Num {
        while amount > Num::ZERO {
            let soonest = self
                .0
                .iter()
                .enumerate()
                .filter(|(_, instance)| instance.shield.is_some_and(|shield| shield > Num::ZERO))
                .filter(|(_, instance)| takes_effect(instance.tags))
                .min_by_key(|&(at, instance)| (instance.until.is_none(), instance.until, at))
                .map(|(at, _)| at);
            let Some(at) = soonest else {
                break;
            };
            let shield = self.0[at]
                .shield
                .as_mut()
                .expect("a shield the search found");
            let spent = (*shield).min(amount);
            *shield -= spent;
            amount -= spent;
            if *shield == Num::ZERO {
                self.0.remove(at);
            }
        }
        amount
    }

    /// Ends every instance a death ends: all but passives.
    pub(crate) fn clear_on_death(&mut self) {
        self.0.retain(|instance| instance.passive);
    }

    /// Ends every instance an aura or a player holds that `holds` no longer keeps; whether any
    /// ended.
    pub(crate) fn release_held(
        &mut self,
        mut holds: impl FnMut(ModifierId, Option<StableId>) -> bool,
    ) -> bool {
        let before = self.0.len();
        self.0
            .retain(|instance| !instance.held || holds(instance.id, instance.source));
        self.0.len() != before
    }

    fn find(&self, id: ModifierId, source: Option<StableId>) -> Result<usize, usize> {
        self.0
            .binary_search_by(|instance| instance.id.cmp(&id).then(instance.source.cmp(&source)))
    }
}

impl Instance {
    /// The first tick a modifier or stack of `ticks` applied in tick `now` no longer holds: it
    /// holds through tick `now + ticks`.
    pub(crate) const fn end(now: Tick, ticks: Ticks) -> Tick {
        now.after(ticks).after(Ticks::ONE)
    }

    /// Takes the numbers and ends of `new`, an application of the same modifier from the same
    /// source, keeping the stacks, when they end, and the script state.
    fn renew(&mut self, new: Instance) {
        let Instance {
            ability,
            rank,
            until,
            stack_life,
            aura_radius,
            shield,
            stats,
            ..
        } = new;
        self.ability = ability;
        self.rank = rank;
        self.until = until;
        self.stack_life = stack_life;
        self.aura_radius = aura_radius;
        self.shield = shield;
        self.stats = stats;
    }

    /// Whether its interval comes in tick `now`; when it does, the next comes an interval later.
    pub(crate) fn interval_due(&mut self, now: Tick) -> bool {
        let Some(interval) = &mut self.interval else {
            return false;
        };
        if interval.next > now {
            return false;
        }
        interval.next = interval.next.after(interval.every);
        true
    }

    /// Writes `stacks` in tick `now`, as a script does. With a stack life, the stacks it takes
    /// away are those that end soonest, and those it adds end as stacks applied now do.
    pub(crate) fn set_stacks(&mut self, stacks: u32, now: Tick) {
        if let Some(life) = self.stack_life {
            if stacks < self.stacks {
                self.end_soonest(self.stacks - stacks);
            } else if stacks > self.stacks {
                self.add_ends(stacks - self.stacks, Instance::end(now, life));
            }
        }
        self.stacks = stacks;
    }

    /// Takes away the ends of the `count` stacks that end soonest.
    fn end_soonest(&mut self, mut count: u32) {
        let mut emptied = 0;
        for end in &mut self.stack_ends {
            let taken = end.count.min(count);
            end.count -= taken;
            count -= taken;
            if end.count > 0 {
                break;
            }
            emptied += 1;
        }
        self.stack_ends.drain(..emptied);
    }

    /// Adds the ends of `count` stacks that end as tick `until` starts.
    fn add_ends(&mut self, count: u32, until: Tick) {
        match self
            .stack_ends
            .binary_search_by_key(&until, |end| end.until)
        {
            Ok(at) => self.stack_ends[at].count += count,
            Err(at) => self.stack_ends.insert(at, StackEnd { until, count }),
        }
    }
}

impl SimComponent for Modifiers {
    const NAME: &'static str = "stats.modifiers";
}

/// A snapshot is untrusted, so instances out of order or twice, stack ends out of order, empty,
/// or that do not count an instance's stacks, and an interval of no ticks fail to decode.
impl<'de> Deserialize<'de> for Modifiers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Modifiers, D::Error> {
        let instances = Vec::<Instance>::deserialize(deserializer)?;
        let ordered = instances
            .windows(2)
            .all(|pair| (pair[0].id, pair[0].source) < (pair[1].id, pair[1].source));
        let stacks = instances.iter().all(|instance| {
            let ends = &instance.stack_ends;
            let ordered = ends.windows(2).all(|pair| pair[0].until < pair[1].until);
            let counted: u64 = ends.iter().map(|end| u64::from(end.count)).sum();
            let counts = match instance.stack_life {
                Some(_) => counted == u64::from(instance.stacks),
                None => ends.is_empty(),
            };
            let interval = instance
                .interval
                .is_none_or(|interval| interval.every > Ticks::ZERO);
            ordered && counts && interval && ends.iter().all(|end| end.count > 0)
        });
        if !ordered || !stacks {
            return Err(D::Error::custom(
                "modifiers out of order or with stray stack ends",
            ));
        }
        Ok(Modifiers(instances))
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    /// An application of modifier `id` from `source`, reapplied as `reapply`, up to
    /// `max_stacks`, adding `armor` a stack, holding until tick `until`, and with `stack` =
    /// `(now, life)`, its stack applied in tick `now` for `life` ticks.
    fn applied(
        id: u16,
        source: Option<StableId>,
        reapply: Reapply,
        max_stacks: Option<u32>,
        armor: i64,
        until: Option<u64>,
        stack: Option<(u64, u64)>,
    ) -> Application {
        let stack_life = stack.map(|(_, life)| Ticks::new(life));
        let stack_ends = stack.map(|(now, life)| StackEnd {
            until: Instance::end(Tick::new(now), Ticks::new(life)),
            count: 1,
        });
        Application {
            instance: Instance {
                id: ModifierId::new(id),
                source,
                ability: None,
                rank: 1,
                passive: false,
                held: false,
                aura_radius: None,
                stacks: 1,
                until: until.map(Tick::new),
                stack_life,
                stack_ends: stack_ends.into_iter().collect(),
                interval: None,
                shield: None,
                stats: vec![StatShare {
                    stat: 0,
                    op: StatOp::Add,
                    value: num(armor),
                    live: None,
                }],
                tags: TagSet::default(),
                state: vec![StateValue::Int(7)],
            },
            reapply,
            max_stacks: max_stacks.and_then(NonZeroU32::new),
        }
    }

    fn stacks(modifiers: &Modifiers) -> Vec<(u16, u32)> {
        modifiers
            .iter()
            .map(|instance| (u16::try_from(instance.id.index()).unwrap(), instance.stacks))
            .collect()
    }

    fn ends(instance: &Instance) -> Vec<(u64, u32)> {
        let ends = instance.stack_ends.iter();
        ends.map(|end| (end.until.get(), end.count)).collect()
    }

    #[test]
    fn modifiers_refresh_stack_to_their_limit_ignore_and_end() {
        let mut ids = IdAllocator::default();
        let (a, b) = (Some(ids.allocate()), Some(ids.allocate()));
        let mut modifiers = Modifiers::default();
        // Refreshing takes the new numbers and end, and keeps the stacks and state: armor 5 to
        // 10, the end 10 to 20.
        modifiers.apply(applied(0, a, Reapply::Refresh, None, 5, Some(10), None));
        modifiers.get_mut(ModifierId::new(0), a).unwrap().state[0] = StateValue::Int(9);
        modifiers.apply(applied(0, a, Reapply::Refresh, None, 10, Some(20), None));
        let held = modifiers.get(ModifierId::new(0), a).unwrap();
        assert_eq!((held.stacks, held.until), (1, Some(Tick::new(20))));
        assert_eq!(
            (held.stats[0].value, &held.state[0]),
            (num(10), &StateValue::Int(9))
        );
        // From another source, another instance, kept after the first by source.
        modifiers.apply(applied(0, b, Reapply::Refresh, None, 1, None, None));
        // Stacking to a limit of 3, each stack for 3 ticks, applied in ticks 0 to 3: 1, 2, 3,
        // then 3 stacks; they end as ticks 4, 5 and 6 start, and the fourth, ending as 7
        // starts, takes the place of the one that ends soonest.
        for (now, stacks) in [(0, 1), (1, 2), (2, 3), (3, 3)] {
            let stack = Some((now, 3));
            modifiers.apply(applied(1, a, Reapply::Stack, Some(3), 2, None, stack));
            assert_eq!(modifiers.get(ModifierId::new(1), a).unwrap().stacks, stacks);
        }
        let held = modifiers.get(ModifierId::new(1), a).unwrap();
        assert_eq!(ends(held), [(5, 1), (6, 1), (7, 1)]);
        // Ignoring leaves it as it was.
        modifiers.apply(applied(2, None, Reapply::Ignore, None, 3, Some(8), None));
        modifiers.apply(applied(2, None, Reapply::Ignore, None, 30, Some(80), None));
        let ignored = modifiers.get(ModifierId::new(2), None).unwrap();
        assert_eq!(
            (ignored.stats[0].value, ignored.until),
            (num(3), Some(Tick::new(8)))
        );
        assert_eq!(stacks(&modifiers), [(0, 1), (0, 1), (1, 3), (2, 1)]);

        // As tick 5 starts nothing has ended but the first stack; as 6 starts, the second; as 7
        // starts the third, and as 8 the last, which ends its instance, and the ignored one.
        assert!(modifiers.expire(Tick::new(5)));
        assert_eq!(stacks(&modifiers), [(0, 1), (0, 1), (1, 2), (2, 1)]);
        assert!(!modifiers.expire(Tick::new(5)));
        assert!(modifiers.expire(Tick::new(8)));
        assert_eq!(stacks(&modifiers), [(0, 1), (0, 1)]);

        // A stack of a shorter life, as at another rank, ends in its order: applied in tick 10
        // for 20 ticks, it ends as 31 starts; in tick 11 for 2, as 14 starts.
        modifiers.apply(applied(1, a, Reapply::Stack, None, 2, None, Some((10, 20))));
        modifiers.apply(applied(1, a, Reapply::Stack, None, 2, None, Some((11, 2))));
        let instance = modifiers.get_mut(ModifierId::new(1), a).unwrap();
        assert_eq!(ends(instance), [(14, 1), (31, 1)]);
        // Writing 4 stacks in tick 12 adds two that end as stacks of 2 ticks applied then do,
        // as 15 starts; writing 3 takes away the one that ends soonest.
        instance.set_stacks(4, Tick::new(12));
        assert_eq!(ends(instance), [(14, 1), (15, 2), (31, 1)]);
        instance.set_stacks(3, Tick::new(12));
        assert_eq!(ends(instance), [(15, 2), (31, 1)]);
        // As 15 starts two of the 3 end; writing 0 then keeps the instance, with no ends.
        assert!(modifiers.expire(Tick::new(15)));
        let instance = modifiers.get_mut(ModifierId::new(1), a).unwrap();
        assert_eq!((instance.stacks, ends(instance)), (1, vec![(31, 1)]));
        instance.set_stacks(0, Tick::new(16));
        assert_eq!((instance.stacks, ends(instance)), (0, vec![]));

        // A passive whose last stack ends stays, with none: one stack of 2 ticks applied in
        // tick 16 ends as 19 starts.
        modifiers.apply(applied(3, a, Reapply::Stack, None, 1, None, Some((16, 2))));
        modifiers.get_mut(ModifierId::new(3), a).unwrap().passive = true;
        assert!(modifiers.expire(Tick::new(19)));
        assert_eq!(modifiers.get(ModifierId::new(3), a).unwrap().stacks, 0);

        // Removing one, then a death, which keeps only passives.
        assert!(modifiers.remove(ModifierId::new(0), b));
        assert!(!modifiers.remove(ModifierId::new(0), b));
        modifiers.get_mut(ModifierId::new(0), a).unwrap().passive = true;
        modifiers.clear_on_death();
        assert_eq!(stacks(&modifiers), [(0, 1), (3, 0)]);
    }

    #[test]
    fn modifiers_decode_only_with_ordered_ends_that_count_their_stacks() {
        let decode = |instance: &Instance| {
            let bytes = postcard::to_allocvec(&Modifiers(vec![instance.clone()])).unwrap();
            postcard::from_bytes::<Modifiers>(&bytes).is_ok()
        };
        let mut instance = applied(0, None, Reapply::Stack, None, 1, None, Some((0, 4))).instance;
        instance.stacks = 3;
        instance.stack_ends = vec![
            StackEnd {
                until: Tick::new(5),
                count: 1,
            },
            StackEnd {
                until: Tick::new(9),
                count: 2,
            },
        ];
        assert!(decode(&instance));
        let mut uncounted = instance.clone();
        uncounted.stacks = 4;
        let mut unordered = instance.clone();
        unordered.stack_ends.swap(0, 1);
        let mut empty = instance.clone();
        empty.stack_ends[0].count = 0;
        empty.stacks = 2;
        let mut lifeless = instance.clone();
        lifeless.stack_life = None;
        for bad in [uncounted, unordered, empty, lifeless] {
            assert!(!decode(&bad), "{bad:?}");
        }
    }
}
