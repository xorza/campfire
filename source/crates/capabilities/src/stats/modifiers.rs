use std::ops::{Deref, Range};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Tick};
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::stats::application::Application;
use crate::stats::application::NewInstance;
use crate::stats::instance::Instance;
use crate::stats::instance::StackEnd;
use crate::stats::instance::StatShare;
use crate::stats::lifetime::{Ends, Hold};
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_data::Reapply;
use crate::stats::param_book::ParamBook;
use crate::units::modifier_id::ModifierId;

/// The modifiers a unit carries, by id, then source, one instance of an id from each source,
/// and what each adds to its stats: a change here derives the unit's stats again. Each
/// instance's stack ends and stat shares sit in one buffer each, instance after instance, so a
/// copy of a unit's modifiers, as a rollback makes, allocates a buffer per kind, not per
/// instance. Its clocks, which derive nothing, are `ModifierClocks`, in the same order.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[require(ModifierClocks)]
pub struct Modifiers {
    instances: Vec<Instance>,
    ends: Vec<StackEnd>,
    shares: Vec<StatShare>,
}

/// An instance a unit carries, with its runs of the carrier's buffers: with a stack life, when
/// its stacks end, by tick, ascending, their counts adding to its stacks, and none without one;
/// and what it adds a stack to each stat its modifier changes, in the order of its modifier's
/// changes, which name the stats and how they change.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Carried<'a> {
    pub(crate) instance: &'a Instance,
    pub(crate) stack_ends: &'a [StackEnd],
    pub(crate) shares: &'a [StatShare],
}

/// Which of a unit's modifier components an operation changed: a change of `stats` derives the
/// unit's stats again, one of `clocks` does not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Touched {
    pub(crate) stats: bool,
    pub(crate) clocks: bool,
}

/// Where an instance's runs start in its carrier's buffers.
#[derive(Debug, Clone, Copy, Default)]
struct Starts {
    ends: usize,
    shares: usize,
}

impl Modifiers {
    pub(crate) fn iter(&self) -> impl Iterator<Item = Carried<'_>> + Clone {
        let mut starts = Starts::default();
        self.instances.iter().map(move |instance| {
            let carried = self.carried(instance, starts);
            starts.ends += usize::from(instance.ends);
            starts.shares += usize::from(instance.shares);
            carried
        })
    }

    pub(crate) const fn len(&self) -> usize {
        self.instances.len()
    }

    /// The instance of `id` from `source`.
    pub(crate) fn get(&self, id: ModifierId, source: Option<StableId>) -> Option<Carried<'_>> {
        let at = self.find(id, source).ok()?;
        Some(self.carried(&self.instances[at], self.starts(at)))
    }

    /// The place of the instance of `id` from `source` among them, and among its clocks.
    pub(crate) fn position(&self, id: ModifierId, source: Option<StableId>) -> Option<usize> {
        self.find(id, source).ok()
    }

    /// Writes `value` as the value of the share at `at` of its buffer, as a live change last
    /// computed it.
    pub(crate) fn set_share_value(&mut self, at: usize, value: Num) {
        self.shares[at].value = value;
    }

    /// Applies `application`: a new instance, its clock among `clocks`, or one more application
    /// of the instance its source holds, as its `reapply` says. `refresh` takes the new numbers,
    /// shield and end and keeps the stacks; `stack` does too, and adds a stack up to the limit, a
    /// stack past it ending the stack that ends soonest in its place; `ignore` leaves the
    /// instance. The script state stays.
    pub(crate) fn apply(
        &mut self,
        clocks: &mut ModifierClocks,
        application: Application,
    ) -> Touched {
        let Application {
            instance: new,
            reapply,
            max_stacks,
        } = application;
        let at = match self.find(new.id, new.source) {
            Ok(at) => at,
            Err(at) => {
                self.insert(clocks, at, new);
                return Touched {
                    stats: true,
                    clocks: true,
                };
            }
        };
        match reapply {
            Reapply::Ignore => Touched::default(),
            Reapply::Refresh => {
                self.renew(clocks, at, new);
                Touched {
                    stats: true,
                    clocks: true,
                }
            }
            Reapply::Stack => {
                let stacks = reapply.stacks(self.instances[at].stacks, max_stacks);
                let added = new.stack_ends.first().map(|end| end.until);
                self.renew(clocks, at, new);
                if let Some(until) = added {
                    if stacks == self.instances[at].stacks {
                        self.end_soonest(at, 1);
                    }
                    self.add_ends(at, 1, until);
                }
                self.instances[at].stacks = stacks;
                Touched {
                    stats: true,
                    clocks: true,
                }
            }
        }
    }

    /// Lets `hold` keep the instance of `id` from `source` too, which it carries.
    pub(crate) fn hold(&mut self, id: ModifierId, source: Option<StableId>, hold: Hold) {
        let at = self.find(id, source).expect("an instance it carries");
        self.instances[at].lifetime.hold(hold);
    }

    /// Lets go of `hold` on the instance of `id` from `source`, which ends, with its clock, when
    /// nothing else keeps it.
    pub(crate) fn release(
        &mut self,
        clocks: &mut ModifierClocks,
        id: ModifierId,
        source: Option<StableId>,
        hold: Hold,
    ) {
        if let Ok(at) = self.find(id, source)
            && !self.instances[at].lifetime.release(hold)
        {
            self.remove_at(clocks, at);
        }
    }

    /// Removes the instance of `id` from `source`, with its clock; whether it held one.
    pub(crate) fn remove(
        &mut self,
        clocks: &mut ModifierClocks,
        id: ModifierId,
        source: Option<StableId>,
    ) -> bool {
        let Ok(at) = self.find(id, source) else {
            return false;
        };
        self.remove_at(clocks, at);
        true
    }

    /// Writes `stacks` of the instance of `id` from `source` in tick `now`, as a script does,
    /// if it carries one. With a stack life, the stacks it takes away are those that end
    /// soonest, and those it adds end as stacks applied now do.
    pub(crate) fn set_stacks(
        &mut self,
        id: ModifierId,
        source: Option<StableId>,
        stacks: u32,
        now: Tick,
    ) {
        let Ok(at) = self.find(id, source) else {
            return;
        };
        let instance = self.instances[at];
        if let Some(life) = instance.stack_life {
            if stacks < instance.stacks {
                self.end_soonest(at, instance.stacks - stacks);
            } else if stacks > instance.stacks {
                self.add_ends(at, stacks - instance.stacks, Instance::end(now, life));
            }
        }
        self.instances[at].stacks = stacks;
    }

    /// Ends what no longer holds as tick `now` starts: each stack whose end it reached, then
    /// each instance, with its clock, that nothing keeps once its application's end is reached,
    /// or whose stacks ending one by one all ended, but a passive, which stays with none;
    /// whether any ended.
    pub(crate) fn expire(&mut self, clocks: &mut ModifierClocks, now: Tick) -> Touched {
        let reached = self.iter().any(|carried| {
            matches!(carried.instance.lifetime.applied, Some(Ends::At(until)) if until <= now)
                || carried
                    .stack_ends
                    .first()
                    .is_some_and(|end| end.until <= now)
        });
        if !reached {
            return Touched::default();
        }
        let mut removed = false;
        let mut starts = Starts::default();
        let mut at = 0;
        while at < self.instances.len() {
            let ends = starts.ends..starts.ends + usize::from(self.instances[at].ends);
            let ended = self.ends[ends].partition_point(|end| end.until <= now);
            let mut emptied = false;
            if ended > 0 {
                let count: u32 = self
                    .ends
                    .drain(starts.ends..starts.ends + ended)
                    .map(|end| end.count)
                    .sum();
                let instance = &mut self.instances[at];
                instance.ends -= u16::try_from(ended).expect("a run's length fits u16");
                instance.stacks -= count;
                emptied = instance.stacks == 0;
            }
            let instance = &mut self.instances[at];
            let keeps = if emptied {
                instance.lifetime.held_by(Hold::Passive)
            } else {
                instance.lifetime.lasts(now)
            };
            if keeps {
                starts.ends += usize::from(instance.ends);
                starts.shares += usize::from(instance.shares);
                at += 1;
            } else {
                self.remove_run(clocks, at, starts);
                removed = true;
            }
        }
        Touched {
            stats: true,
            clocks: removed,
        }
    }

    /// Spends shields among `clocks` on `amount` of damage: of the instances whose modifier
    /// `takes_effect` lets act, the shield that ends soonest first, one with no end last, and
    /// shields with the same end in the order kept; a shield spent to 0 ends its instance. What
    /// is left of the amount, and what it changed.
    pub(crate) fn absorb(
        &mut self,
        clocks: &mut ModifierClocks,
        mut amount: Num,
        takes_effect: impl Fn(ModifierId) -> bool,
    ) -> Absorbed {
        let mut touched = Touched::default();
        while amount > Num::ZERO {
            let soonest = self
                .instances
                .iter()
                .enumerate()
                .filter(|&(at, _)| clocks.shield(at).is_some_and(|shield| shield > Num::ZERO))
                .filter(|(_, instance)| takes_effect(instance.id))
                .min_by_key(|&(at, instance)| {
                    let until = instance.lifetime.until();
                    (until.is_none(), until, at)
                })
                .map(|(at, _)| at);
            let Some(at) = soonest else {
                break;
            };
            let spent = clocks.spend_shield(at, amount);
            amount -= spent.amount;
            touched.clocks = true;
            if spent.emptied {
                self.remove_at(clocks, at);
                touched.stats = true;
            }
        }
        Absorbed {
            left: amount,
            touched,
        }
    }

    /// Ends every instance a death ends, with its clock: all but passives.
    pub(crate) fn clear_on_death(&mut self, clocks: &mut ModifierClocks) {
        self.remove_where(clocks, |instance| !instance.lifetime.held_by(Hold::Passive));
    }

    /// Lets go of the hold of every instance an aura, an area or a player holds that `holds` no
    /// longer keeps, each ending, with its clock, when nothing else keeps it; whether any
    /// changed.
    pub(crate) fn release_held(
        &mut self,
        clocks: &mut ModifierClocks,
        mut holds: impl FnMut(ModifierId, Option<StableId>) -> bool,
    ) -> bool {
        let mut changed = false;
        self.remove_where(clocks, |instance| {
            if !instance.lifetime.held_by(Hold::Held) || holds(instance.id, instance.source) {
                return false;
            }
            changed = true;
            !instance.lifetime.release(Hold::Held)
        });
        changed
    }

    /// Removes, with their clocks, the instances `ends` says end, which it may change.
    fn remove_where(
        &mut self,
        clocks: &mut ModifierClocks,
        mut ends: impl FnMut(&mut Instance) -> bool,
    ) {
        let mut starts = Starts::default();
        let mut at = 0;
        while at < self.instances.len() {
            if ends(&mut self.instances[at]) {
                self.remove_run(clocks, at, starts);
            } else {
                starts.ends += usize::from(self.instances[at].ends);
                starts.shares += usize::from(self.instances[at].shares);
                at += 1;
            }
        }
    }

    fn find(&self, id: ModifierId, source: Option<StableId>) -> Result<usize, usize> {
        self.instances
            .binary_search_by(|instance| instance.id.cmp(&id).then(instance.source.cmp(&source)))
    }

    /// Where the runs of the instance at `at` start.
    fn starts(&self, at: usize) -> Starts {
        self.instances[..at]
            .iter()
            .fold(Starts::default(), |starts, instance| Starts {
                ends: starts.ends + usize::from(instance.ends),
                shares: starts.shares + usize::from(instance.shares),
            })
    }

    fn carried<'a>(&'a self, instance: &'a Instance, starts: Starts) -> Carried<'a> {
        Carried {
            instance,
            stack_ends: &self.ends[starts.ends..starts.ends + usize::from(instance.ends)],
            shares: &self.shares[starts.shares..starts.shares + usize::from(instance.shares)],
        }
    }

    fn ends_range(&self, at: usize) -> Range<usize> {
        let start = self.starts(at).ends;
        start..start + usize::from(self.instances[at].ends)
    }

    /// Puts `new` at `at`, and its clock among `clocks`.
    fn insert(&mut self, clocks: &mut ModifierClocks, at: usize, new: NewInstance) {
        let starts = self.starts(at);
        let run = |len: usize| u16::try_from(len).expect("an instance's run fits u16");
        let instance = Instance {
            id: new.id,
            source: new.source,
            ability: new.ability,
            rank: new.rank,
            lifetime: new.lifetime,
            aura_radius: new.aura_radius,
            stacks: new.stacks,
            stack_life: new.stack_life,
            ends: run(new.stack_ends.len()),
            shares: run(new.stats.len()),
        };
        self.ends.splice(starts.ends..starts.ends, new.stack_ends);
        self.shares.splice(starts.shares..starts.shares, new.stats);
        self.instances.insert(at, instance);
        clocks.insert(at, new.interval, new.shield, new.state);
    }

    fn remove_at(&mut self, clocks: &mut ModifierClocks, at: usize) {
        let starts = self.starts(at);
        self.remove_run(clocks, at, starts);
    }

    /// Removes the instance at `at`, whose runs start at `starts`, and its clock.
    fn remove_run(&mut self, clocks: &mut ModifierClocks, at: usize, starts: Starts) {
        let instance = self.instances.remove(at);
        self.ends
            .drain(starts.ends..starts.ends + usize::from(instance.ends));
        self.shares
            .drain(starts.shares..starts.shares + usize::from(instance.shares));
        clocks.remove(at);
    }

    /// Takes the numbers of `new`, an application of the same modifier from the same source, to
    /// the instance at `at`, and joins its lifetime, keeping the stacks, when they end, and the
    /// script state. Its interval takes the new length from the next on, and the next keeps its
    /// tick, so no refresh puts it off.
    fn renew(&mut self, clocks: &mut ModifierClocks, at: usize, new: NewInstance) {
        let starts = self.starts(at);
        let held = &mut self.instances[at];
        held.ability = new.ability;
        held.rank = new.rank;
        held.lifetime = held.lifetime.joined(new.lifetime);
        held.stack_life = new.stack_life;
        held.aura_radius = new.aura_radius;
        let shares = starts.shares..starts.shares + usize::from(held.shares);
        held.shares = u16::try_from(new.stats.len()).expect("an instance's run fits u16");
        self.shares.splice(shares, new.stats);
        clocks.renew(at, new.interval, new.shield);
    }

    /// Takes away the ends of the `count` stacks that end soonest of the instance at `at`.
    fn end_soonest(&mut self, at: usize, mut count: u32) {
        let ends = self.ends_range(at);
        let mut emptied = 0;
        for end in &mut self.ends[ends.clone()] {
            let taken = end.count.min(count);
            end.count -= taken;
            count -= taken;
            if end.count > 0 {
                break;
            }
            emptied += 1;
        }
        self.ends.drain(ends.start..ends.start + emptied);
        self.instances[at].ends -= u16::try_from(emptied).expect("a run's length fits u16");
    }

    /// Adds to the instance at `at` the ends of `count` stacks that end as tick `until` starts.
    fn add_ends(&mut self, at: usize, count: u32, until: Tick) {
        let ends = self.ends_range(at);
        match self.ends[ends.clone()].binary_search_by_key(&until, |end| end.until) {
            Ok(found) => self.ends[ends.start + found].count += count,
            Err(found) => {
                self.ends
                    .insert(ends.start + found, StackEnd { until, count });
                self.instances[at].ends += 1;
            }
        }
    }
}

/// What spending shields left of an amount, and which components it changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Absorbed {
    pub(crate) left: Num,
    pub(crate) touched: Touched,
}

/// A carried instance reads as its instance, beside its runs.
impl Deref for Carried<'_> {
    type Target = Instance;

    fn deref(&self) -> &Instance {
        self.instance
    }
}

impl Carried<'_> {
    /// Whether its modifier, its ability's params and each live param it reads are ones the books
    /// hold, and it has a value for each of its modifier's stat changes.
    pub(crate) fn fits(&self, modifiers: &ModifierBook, params: &ParamBook) -> bool {
        let Some(entry) = modifiers.entry(self.instance.id) else {
            return false;
        };
        let ability = self
            .instance
            .ability
            .is_none_or(|ability| params.has_action(ability));
        let shares = self.shares.len() == entry.spec.stats.len()
            && self
                .shares
                .iter()
                .all(|share| share.live.is_none_or(|live| params.has_live(live)));
        ability && shares
    }
}

impl SimComponent for Modifiers {
    const NAME: &'static str = "stats.modifiers";

    // A modifier, a param the books lack, or another count of shares than its modifier's changes,
    // would be read past the books' places; its clocks are one for each instance.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let books = (
            world.get_resource::<ModifierBook>(),
            world.get_resource::<ParamBook>(),
        );
        let clocks = world
            .get::<ModifierClocks>(entity)
            .is_some_and(|clocks| clocks.len() == self.instances.len());
        let (Some(modifiers), Some(params)) = books else {
            return self.instances.is_empty() && clocks;
        };
        clocks && self.iter().all(|carried| carried.fits(modifiers, params))
    }
}

/// A snapshot is untrusted, so instances out of order or twice, runs that do not cover the
/// buffers, stack ends out of order, empty, or that do not count an instance's stacks, and a
/// negative aura radius fail to decode.
impl<'de> Deserialize<'de> for Modifiers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Modifiers, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            instances: Vec<Instance>,
            ends: Vec<StackEnd>,
            shares: Vec<StatShare>,
        }
        let Fields {
            instances,
            ends,
            shares,
        } = Fields::deserialize(deserializer)?;
        let total = |count: fn(&Instance) -> u16| {
            instances
                .iter()
                .map(|instance| usize::from(count(instance)))
                .sum::<usize>()
        };
        if total(|instance| instance.ends) != ends.len()
            || total(|instance| instance.shares) != shares.len()
        {
            return Err(D::Error::custom("runs that cover the buffers exactly"));
        }
        let modifiers = Modifiers {
            instances,
            ends,
            shares,
        };
        let ordered = modifiers
            .instances
            .windows(2)
            .all(|pair| (pair[0].id, pair[0].source) < (pair[1].id, pair[1].source));
        let stacks = modifiers.iter().all(|carried| {
            let (instance, ends) = (carried.instance, carried.stack_ends);
            let ordered = ends.windows(2).all(|pair| pair[0].until < pair[1].until);
            let counted: u64 = ends.iter().map(|end| u64::from(end.count)).sum();
            let counts = match instance.stack_life {
                Some(_) => counted == u64::from(instance.stacks),
                None => ends.is_empty(),
            };
            let radius = instance
                .aura_radius
                .is_none_or(|radius| radius >= Num::ZERO);
            ordered && counts && radius && ends.iter().all(|end| end.count > 0)
        });
        if !ordered || !stacks {
            return Err(D::Error::custom(
                "modifiers out of order or with stray stack ends",
            ));
        }
        Ok(modifiers)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use bevy_ecs::bundle::Bundle;

    use crate::stats::application::Application;
    use crate::stats::modifier_clocks::ModifierClocks;
    use crate::stats::modifiers::Modifiers;

    impl Modifiers {
        /// A unit's modifiers and their clocks, of `applications` applied in order.
        pub(crate) fn bundle(applications: impl IntoIterator<Item = Application>) -> impl Bundle {
            let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
            for application in applications {
                modifiers.apply(&mut clocks, application);
            }
            (modifiers, clocks)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_math::Ticks;
    use campfire_sim::IdAllocator;

    use super::*;
    use crate::scripts::state_value::StateValue;
    use crate::stats::lifetime::Lifetime;
    use crate::stats::modifier_clocks::Interval;

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
            instance: NewInstance {
                id: ModifierId::new(id),
                source,
                ability: None,
                rank: 1,
                aura_radius: None,
                stacks: 1,
                lifetime: Lifetime::new(
                    None,
                    until.map_or(Ends::Never, |until| Ends::At(Tick::new(until))),
                ),
                stack_life,
                stack_ends: stack_ends.into_iter().collect(),
                interval: None,
                shield: None,
                stats: vec![StatShare {
                    value: num(armor),
                    live: None,
                }],
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

    fn ends(carried: Carried<'_>) -> Vec<(u64, u32)> {
        let ends = carried.stack_ends.iter();
        ends.map(|end| (end.until.get(), end.count)).collect()
    }

    #[test]
    fn a_hold_and_a_timed_application_of_one_instance_keep_both_lifetimes() {
        let mut ids = IdAllocator::default();
        let a = Some(ids.allocate());
        let id = ModifierId::new(0);
        let held = || {
            let mut held = applied(0, a, Reapply::Refresh, None, 1, None, None);
            held.instance.lifetime = Lifetime::new(Some(Hold::Held), Ends::Never);
            held
        };
        let timed = || applied(0, a, Reapply::Refresh, None, 1, Some(6), None);
        // Held by an aura, then applied until tick 6: the aura lets go in tick 2, and the
        // application keeps it through tick 5.
        let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
        modifiers.apply(&mut clocks, held());
        modifiers.apply(&mut clocks, timed());
        assert!(modifiers.release_held(&mut clocks, |_, _| false));
        assert!(modifiers.get(id, a).is_some());
        modifiers.expire(&mut clocks, Tick::new(5));
        assert!(modifiers.get(id, a).is_some());
        modifiers.expire(&mut clocks, Tick::new(6));
        assert!(modifiers.get(id, a).is_none());
        // Applied until tick 6, then held: past tick 6 the hold keeps it, with no end; when the
        // aura lets go, it ends.
        let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
        modifiers.apply(&mut clocks, timed());
        modifiers.hold(id, a, Hold::Held);
        modifiers.expire(&mut clocks, Tick::new(8));
        let kept = modifiers.get(id, a).unwrap();
        assert_eq!(kept.lifetime.until(), None);
        assert!(modifiers.release_held(&mut clocks, |_, _| false));
        assert!(modifiers.get(id, a).is_none());
        // A passive whose rank no longer keeps it lets go of its hold alone: an application of
        // its own, with no end, keeps it.
        let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
        modifiers.apply(
            &mut clocks,
            applied(0, a, Reapply::Refresh, None, 1, None, None),
        );
        modifiers.hold(id, a, Hold::Passive);
        modifiers.release(&mut clocks, id, a, Hold::Passive);
        assert!(modifiers.get(id, a).is_some());
    }

    #[test]
    fn an_interval_refreshed_keeps_its_next_tick() {
        let a = Some(IdAllocator::default().allocate());
        // An interval of 3 ticks, next in tick 5, refreshed to 2 ticks, from tick 9: the next
        // stays in tick 5, and the one after comes 2 ticks later, in tick 7.
        let interval = |every, next| {
            let mut application = applied(3, a, Reapply::Refresh, None, 0, None, None);
            application.instance.interval = Some(Interval {
                every: Ticks::new(every),
                next: Tick::new(next),
            });
            application
        };
        let (mut timed, mut timed_clocks) = (Modifiers::default(), ModifierClocks::default());
        timed.apply(&mut timed_clocks, interval(3, 5));
        timed.apply(&mut timed_clocks, interval(2, 9));
        let due = [4, 5, 6, 7]
            .map(|now| timed_clocks.advance_intervals(&timed, Tick::new(now), |_| true, |_, _| {}));
        assert_eq!(due, [false, true, false, true]);
    }

    #[test]
    fn modifiers_refresh_stack_to_their_limit_ignore_and_end() {
        let mut ids = IdAllocator::default();
        let (a, b) = (Some(ids.allocate()), Some(ids.allocate()));
        let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
        // Refreshing takes the new numbers and end, and keeps the stacks and state: armor 5 to
        // 10, the end 10 to 20.
        modifiers.apply(
            &mut clocks,
            applied(0, a, Reapply::Refresh, None, 5, Some(10), None),
        );
        let at = modifiers.position(ModifierId::new(0), a).unwrap();
        clocks.set_state(at, &[StateValue::Int(9)]);
        modifiers.apply(
            &mut clocks,
            applied(0, a, Reapply::Refresh, None, 10, Some(20), None),
        );
        let held = modifiers.get(ModifierId::new(0), a).unwrap();
        assert_eq!(
            (held.stacks, held.lifetime.until()),
            (1, Some(Tick::new(20)))
        );
        assert_eq!(
            (held.shares[0].value, &clocks.state(at)[0]),
            (num(10), &StateValue::Int(9))
        );
        // From another source, another instance, kept after the first by source.
        modifiers.apply(
            &mut clocks,
            applied(0, b, Reapply::Refresh, None, 1, None, None),
        );
        // Stacking to a limit of 3, each stack for 3 ticks, applied in ticks 0 to 3: 1, 2, 3,
        // then 3 stacks; they end as ticks 4, 5 and 6 start, and the fourth, ending as 7
        // starts, takes the place of the one that ends soonest.
        for (now, stacks) in [(0, 1), (1, 2), (2, 3), (3, 3)] {
            let stack = Some((now, 3));
            modifiers.apply(
                &mut clocks,
                applied(1, a, Reapply::Stack, Some(3), 2, None, stack),
            );
            assert_eq!(modifiers.get(ModifierId::new(1), a).unwrap().stacks, stacks);
        }
        let held = modifiers.get(ModifierId::new(1), a).unwrap();
        assert_eq!(ends(held), [(5, 1), (6, 1), (7, 1)]);
        // Ignoring leaves it as it was.
        modifiers.apply(
            &mut clocks,
            applied(2, None, Reapply::Ignore, None, 3, Some(8), None),
        );
        modifiers.apply(
            &mut clocks,
            applied(2, None, Reapply::Ignore, None, 30, Some(80), None),
        );
        let ignored = modifiers.get(ModifierId::new(2), None).unwrap();
        assert_eq!(
            (ignored.shares[0].value, ignored.lifetime.until()),
            (num(3), Some(Tick::new(8)))
        );
        assert_eq!(stacks(&modifiers), [(0, 1), (0, 1), (1, 3), (2, 1)]);

        // As tick 5 starts nothing has ended but the first stack; as 6 starts, the second; as 7
        // starts the third, and as 8 the last, which ends its instance, and the ignored one.
        assert!(modifiers.expire(&mut clocks, Tick::new(5)).stats);
        assert_eq!(stacks(&modifiers), [(0, 1), (0, 1), (1, 2), (2, 1)]);
        assert!(!modifiers.expire(&mut clocks, Tick::new(5)).stats);
        assert!(modifiers.expire(&mut clocks, Tick::new(8)).stats);
        assert_eq!(stacks(&modifiers), [(0, 1), (0, 1)]);

        // A stack of a shorter life, as at another rank, ends in its order: applied in tick 10
        // for 20 ticks, it ends as 31 starts; in tick 11 for 2, as 14 starts.
        modifiers.apply(
            &mut clocks,
            applied(1, a, Reapply::Stack, None, 2, None, Some((10, 20))),
        );
        modifiers.apply(
            &mut clocks,
            applied(1, a, Reapply::Stack, None, 2, None, Some((11, 2))),
        );
        let id = ModifierId::new(1);
        assert_eq!(ends(modifiers.get(id, a).unwrap()), [(14, 1), (31, 1)]);
        // Writing 4 stacks in tick 12 adds two that end as stacks of 2 ticks applied then do,
        // as 15 starts; writing 3 takes away the one that ends soonest.
        modifiers.set_stacks(id, a, 4, Tick::new(12));
        assert_eq!(
            ends(modifiers.get(id, a).unwrap()),
            [(14, 1), (15, 2), (31, 1)]
        );
        modifiers.set_stacks(id, a, 3, Tick::new(12));
        assert_eq!(ends(modifiers.get(id, a).unwrap()), [(15, 2), (31, 1)]);
        // As 15 starts two of the 3 end; writing 0 then keeps the instance, with no ends.
        assert!(modifiers.expire(&mut clocks, Tick::new(15)).stats);
        assert_eq!(
            (
                modifiers.get(id, a).unwrap().stacks,
                ends(modifiers.get(id, a).unwrap())
            ),
            (1, vec![(31, 1)])
        );
        modifiers.set_stacks(id, a, 0, Tick::new(16));
        assert_eq!(
            (
                modifiers.get(id, a).unwrap().stacks,
                ends(modifiers.get(id, a).unwrap())
            ),
            (0, vec![])
        );

        // A passive whose last stack ends stays, with none: one stack of 2 ticks applied in
        // tick 16 ends as 19 starts.
        modifiers.apply(
            &mut clocks,
            applied(3, a, Reapply::Stack, None, 1, None, Some((16, 2))),
        );
        modifiers.hold(ModifierId::new(3), a, Hold::Passive);
        assert!(modifiers.expire(&mut clocks, Tick::new(19)).stats);
        assert_eq!(modifiers.get(ModifierId::new(3), a).unwrap().stacks, 0);

        // Removing one, then a death, which keeps only passives.
        assert!(modifiers.remove(&mut clocks, ModifierId::new(0), b));
        assert!(!modifiers.remove(&mut clocks, ModifierId::new(0), b));
        modifiers.hold(ModifierId::new(0), a, Hold::Passive);
        modifiers.clear_on_death(&mut clocks);
        assert_eq!(stacks(&modifiers), [(0, 1), (3, 0)]);
    }

    #[test]
    fn modifiers_decode_only_with_ordered_ends_that_count_their_stacks() {
        let decode = |instance: &NewInstance| {
            let (mut modifiers, mut clocks) = (Modifiers::default(), ModifierClocks::default());
            let application = Application {
                instance: instance.clone(),
                reapply: Reapply::Refresh,
                max_stacks: None,
            };
            modifiers.apply(&mut clocks, application);
            let bytes = postcard::to_allocvec(&modifiers).unwrap();
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
