use std::ops::{Deref, Range};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
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
mod tests;
