use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_script::{Audience, StateValue};
use campfire_sim::{DueIndex, SimComponent, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::ModifierId;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::interval::Interval;
use crate::interval_revival::IntervalRevival;
use crate::modifier_book::ModifierBook;
use crate::modifiers::Modifiers;

/// What each modifier a unit carries counts and spends besides its stats, in the order of its
/// `Modifiers`: its interval, its shield and its script state. A change here derives nothing
/// again, so an interval that comes, a shield that absorbs or a state a script writes leaves the
/// unit's stats as they are. The state of every instance sits in one buffer, instance after
/// instance. Its hook pushes its unit into the index of the intervals at each insert, a spawn's
/// and a restore's alike.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[component(on_insert = ModifierClocks::pushed)]
pub struct ModifierClocks {
    clocks: Vec<ModifierClock>,
    state: Vec<StateValue>,
}

/// One instance's clock: when its `on_interval` comes, when its modifier has an interval; what
/// is left of its shield; and how many values of state it holds in its carrier's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierClock {
    pub interval: Option<Interval>,
    pub shield: Option<Num>,
    state: u16,
}

/// What a shield spent of an amount, and whether it is empty now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShieldSpent {
    pub(crate) amount: Num,
    pub(crate) emptied: bool,
}

impl ModifierClocks {
    /// The first tick an interval of its instances comes, which `advance_intervals` takes, or
    /// `skip_intervals`; none when none has an interval.
    pub(crate) fn next_interval(&self) -> Option<Tick> {
        let intervals = self.clocks.iter().filter_map(|clock| clock.interval);
        intervals.map(|interval| interval.next).min()
    }

    /// Pushes the unit whose clocks they are into the index of the intervals, at their first.
    fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let (Some(clocks), Some(&id)) = (world.get::<Self>(entity), world.get::<StableId>(entity))
        else {
            return;
        };
        let Some(at) = clocks.next_interval() else {
            return;
        };
        if let Some(mut index) = world.get_resource_mut::<DueIndex<Self>>() {
            index.push(at, id, entity);
        }
    }

    pub const fn len(&self) -> usize {
        self.clocks.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The shield of the instance at `at`.
    pub fn shield(&self, at: usize) -> Option<Num> {
        self.clocks[at].shield
    }

    /// Whether an instance has a shield left to absorb damage.
    pub fn is_shielded(&self) -> bool {
        let left = |clock: &ModifierClock| clock.shield.is_some_and(|shield| shield > Num::ZERO);
        self.clocks.iter().any(left)
    }

    /// The script state of the instance at `at`.
    pub fn state(&self, at: usize) -> &[StateValue] {
        let start = self.state_start(at);
        &self.state[start..start + usize::from(self.clocks[at].state)]
    }

    /// Writes `state`, of its modifier's fields, as the state of the instance at `at`.
    pub(crate) fn set_state(&mut self, at: usize, state: &[StateValue]) {
        let start = self.state_start(at);
        let held = start..start + usize::from(self.clocks[at].state);
        debug_assert_eq!(
            held.len(),
            state.len(),
            "a modifier's state keeps its fields"
        );
        self.state[held].clone_from_slice(state);
    }

    /// Counts the intervals of tick `now`: each instance of `modifiers` whose interval comes,
    /// and whose modifier `takes_effect` lets act, goes to `due`, by id and source, once however
    /// many of its ticks `now` passed, and its next comes at its first tick after `now`; whether
    /// any came.
    pub fn advance_intervals(
        &mut self,
        modifiers: &Modifiers,
        now: Tick,
        takes_effect: impl Fn(ModifierId) -> bool,
        mut due: impl FnMut(ModifierId, Option<StableId>),
    ) -> bool {
        let mut any = false;
        for (clock, carried) in self.clocks.iter_mut().zip(modifiers.iter()) {
            let Some(interval) = &mut clock.interval else {
                continue;
            };
            if interval.next > now {
                continue;
            }
            interval.next = interval.first_after(now);
            let instance = carried.instance;
            if takes_effect(instance.id) {
                due(instance.id, instance.source);
            }
            any = true;
        }
        any
    }

    /// Moves each interval of tick `now` or before to its first tick after `now`, with no hook,
    /// as a dead carrier's intervals keep the clock; whether any moved.
    pub fn skip_intervals(&mut self, now: Tick) -> bool {
        let mut any = false;
        for interval in self
            .clocks
            .iter_mut()
            .filter_map(|clock| clock.interval.as_mut())
        {
            if interval.next <= now {
                interval.next = interval.first_after(now);
                any = true;
            }
        }
        any
    }

    /// Sets each interval of `modifiers` as `revival` gives its modifier's policy, as their
    /// carrier comes back to life in tick `now`: one that resets comes a whole interval on, one
    /// that fires comes in `now`; whether any changed.
    pub fn revive_intervals(
        &mut self,
        modifiers: &Modifiers,
        now: Tick,
        revival: impl Fn(ModifierId) -> IntervalRevival,
    ) -> bool {
        let mut any = false;
        for (clock, carried) in self.clocks.iter_mut().zip(modifiers.iter()) {
            let Some(interval) = &mut clock.interval else {
                continue;
            };
            let next = match revival(carried.instance.id) {
                IntervalRevival::Continue => continue,
                IntervalRevival::Reset => now.after(interval.every),
                IntervalRevival::FireAndReset => now,
            };
            any |= interval.next != next;
            interval.next = next;
        }
        any
    }

    /// Spends the shield of the instance at `at`, which it has, on `amount`.
    pub(crate) fn spend_shield(&mut self, at: usize, amount: Num) -> ShieldSpent {
        let shield = self.clocks[at]
            .shield
            .as_mut()
            .expect("an instance with a shield");
        let spent = (*shield).min(amount);
        *shield -= spent;
        ShieldSpent {
            amount: spent,
            emptied: *shield == Num::ZERO,
        }
    }

    /// Puts the clock of a new instance at `at`.
    pub(crate) fn insert(
        &mut self,
        at: usize,
        interval: Option<Interval>,
        shield: Option<Num>,
        state: impl IntoIterator<Item = StateValue, IntoIter: ExactSizeIterator>,
    ) {
        let state = state.into_iter();
        let start = self.state_start(at);
        let clock = ModifierClock {
            interval,
            shield,
            state: u16::try_from(state.len()).expect("a modifier's state fits u16"),
        };
        self.state.splice(start..start, state);
        self.clocks.insert(at, clock);
    }

    /// Removes the clock at `at`, with its state.
    pub fn remove(&mut self, at: usize) {
        let start = self.state_start(at);
        let clock = self.clocks.remove(at);
        self.state.drain(start..start + usize::from(clock.state));
    }

    /// Takes the shield of a new application of the instance at `at`, and its interval's new
    /// length from the next on: the next keeps its tick, so no refresh puts it off.
    pub(crate) fn renew(&mut self, at: usize, interval: Option<Interval>, shield: Option<Num>) {
        let clock = &mut self.clocks[at];
        debug_assert_eq!(
            clock.interval.is_some(),
            interval.is_some(),
            "a modifier has an interval or not"
        );
        if let (Some(held), Some(new)) = (&mut clock.interval, interval) {
            held.every = new.every;
        }
        clock.shield = shield;
    }

    fn state_start(&self, at: usize) -> usize {
        self.clocks[..at]
            .iter()
            .map(|clock| usize::from(clock.state))
            .sum()
    }
}

/// A unit is due as the first interval of its modifiers comes
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for ModifierClocks {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), clocks: &Self) -> Option<Tick> {
        clocks.next_interval()
    }
}

impl SimComponent for ModifierClocks {
    const NAME: &'static str = "stats.modifier_clocks";
    type Host = UnitEntity;

    // A clock for an instance its unit lacks, or state of other fields than its modifier's,
    // would be read past the instance's or the modifier's places; and an interval or a shield its
    // modifier lacks, or none where it has one, would act on what the modifier does not do.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<ModifierBook>();
        let Some(modifiers) = world.get::<Modifiers>(entity) else {
            return self.clocks.is_empty();
        };
        modifiers.len() == self.clocks.len()
            && modifiers.iter().enumerate().all(|(at, carried)| {
                book.entry(carried.instance.id).is_some_and(|entry| {
                    let spec = &entry.spec;
                    let clock = &self.clocks[at];
                    let state = self.state(at);
                    clock.interval.is_some() == spec.interval.is_some()
                        && clock.shield.is_some() == spec.shield.is_some()
                        && spec.state.holds(Audience::None, state)
                })
            })
    }
}

impl Replication for ModifierClocks {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so runs of state that do not cover the buffer, an interval of no
/// ticks, and a negative shield fail to decode.
impl<'de> Deserialize<'de> for ModifierClocks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            clocks: Vec<ModifierClock>,
            state: Vec<StateValue>,
        }
        let Fields { clocks, state } = Fields::deserialize(deserializer)?;
        let covered: usize = clocks.iter().map(|clock| usize::from(clock.state)).sum();
        let numbers = clocks.iter().all(|clock| {
            clock
                .interval
                .is_none_or(|interval| interval.every > Ticks::ZERO)
                && clock.shield.is_none_or(|shield| shield >= Num::ZERO)
        });
        if covered != state.len() || !numbers {
            return Err(D::Error::custom(
                "clocks that do not cover their state, or out of their limits",
            ));
        }
        Ok(Self { clocks, state })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;
    use campfire_sim::StableId;
    use campfire_values::ModifierId;

    use crate::modifiers::Modifiers;
    use crate::modifiers::modifier_clocks::{ModifierClock, ModifierClocks};

    impl ModifierClocks {
        /// The shield of the instance of `id` from `source` of `modifiers`, if it carries one.
        pub fn shield_of(
            &self,
            modifiers: &Modifiers,
            id: ModifierId,
            source: Option<StableId>,
        ) -> Option<Num> {
            self.shield(modifiers.position(id, source)?)
        }

        /// The clock of the instance at `at`, to change as a flawed snapshot would.
        pub fn clock_mut(&mut self, at: usize) -> &mut ModifierClock {
            &mut self.clocks[at]
        }
    }
}
