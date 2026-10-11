use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::scripts::state_value::StateValue;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifiers::Modifiers;
use crate::units::modifier_id::ModifierId;

/// What each modifier a unit carries counts and spends besides its stats, in the order of its
/// `Modifiers`: its interval, its shield and its script state. A change here derives nothing
/// again, so an interval that comes, a shield that absorbs or a state a script writes leaves the
/// unit's stats as they are. The state of every instance sits in one buffer, instance after
/// instance.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ModifierClocks {
    clocks: Vec<Clock>,
    state: Vec<StateValue>,
}

/// One instance's clock: when its `on_interval` comes, when its modifier has an interval; what
/// is left of its shield; and how many values of state it holds in its carrier's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Clock {
    pub(crate) interval: Option<Interval>,
    pub(crate) shield: Option<Num>,
    state: u16,
}

/// A modifier's interval: every `every` ticks, the next in tick `next`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Interval {
    pub(crate) every: Ticks,
    pub(crate) next: Tick,
}

/// What a shield spent of an amount, and whether it is empty now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Spent {
    pub(crate) amount: Num,
    pub(crate) emptied: bool,
}

impl ModifierClocks {
    pub(crate) const fn len(&self) -> usize {
        self.clocks.len()
    }

    /// The shield of the instance at `at`.
    pub(crate) fn shield(&self, at: usize) -> Option<Num> {
        self.clocks[at].shield
    }

    /// Whether an instance has a shield left to absorb damage.
    pub(crate) fn shielded(&self) -> bool {
        let left = |clock: &Clock| clock.shield.is_some_and(|shield| shield > Num::ZERO);
        self.clocks.iter().any(left)
    }

    /// The script state of the instance at `at`.
    pub(crate) fn state(&self, at: usize) -> &[StateValue] {
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
    /// and whose modifier `takes_effect` lets act, goes to `due`, by id and source, and its next
    /// comes an interval later; whether any came.
    pub(crate) fn advance_intervals(
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
            interval.next = interval.next.after(interval.every);
            let instance = carried.instance;
            if takes_effect(instance.id) {
                due(instance.id, instance.source);
            }
            any = true;
        }
        any
    }

    /// Spends the shield of the instance at `at`, which it has, on `amount`.
    pub(crate) fn spend_shield(&mut self, at: usize, amount: Num) -> Spent {
        let shield = self.clocks[at]
            .shield
            .as_mut()
            .expect("an instance with a shield");
        let spent = (*shield).min(amount);
        *shield -= spent;
        Spent {
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
        state: Vec<StateValue>,
    ) {
        let start = self.state_start(at);
        let clock = Clock {
            interval,
            shield,
            state: u16::try_from(state.len()).expect("a modifier's state fits u16"),
        };
        self.state.splice(start..start, state);
        self.clocks.insert(at, clock);
    }

    /// Removes the clock at `at`, with its state.
    pub(crate) fn remove(&mut self, at: usize) {
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

impl SimComponent for ModifierClocks {
    const NAME: &'static str = "stats.modifier_clocks";

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
                        && state.len() == spec.fields.len()
                        && state
                            .iter()
                            .zip(spec.fields.iter())
                            .all(|(value, field)| value.kind() == field.kind)
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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModifierClocks, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            clocks: Vec<Clock>,
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
        Ok(ModifierClocks { clocks, state })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_math::Num;
    use campfire_sim::StableId;

    use crate::stats::modifier_clocks::{Clock, ModifierClocks};
    use crate::stats::modifiers::Modifiers;
    use crate::units::modifier_id::ModifierId;

    impl ModifierClocks {
        /// The shield of the instance of `id` from `source` of `modifiers`, if it carries one.
        pub(crate) fn shield_of(
            &self,
            modifiers: &Modifiers,
            id: ModifierId,
            source: Option<StableId>,
        ) -> Option<Num> {
            self.shield(modifiers.position(id, source)?)
        }

        /// The clock of the instance at `at`, to change as a flawed snapshot would.
        pub(crate) fn clock_mut(&mut self, at: usize) -> &mut Clock {
            &mut self.clocks[at]
        }
    }
}
