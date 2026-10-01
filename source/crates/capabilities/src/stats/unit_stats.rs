use bevy_ecs::component::Component;
use campfire_math::Num;

use crate::stats::unit_states::UnitStates;

/// A unit's stats, each the mode declares in the stat book's order, and its states: derived
/// from its type, level and modifiers whenever they change, never state.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UnitStats {
    values: Vec<Num>,
    states: UnitStates,
}

impl UnitStats {
    pub(crate) const fn states(&self) -> UnitStates {
        self.states
    }

    /// The states of a unit with `stats`: none for one with no stats.
    pub(crate) fn states_of(stats: Option<&UnitStats>) -> UnitStates {
        stats.map_or_else(UnitStates::default, UnitStats::states)
    }

    pub(crate) const fn set_states(&mut self, states: UnitStates) {
        self.states = states;
    }

    pub(crate) fn values(&self) -> &[Num] {
        &self.values
    }

    /// The values to fill again, cleared.
    pub(crate) fn refill(&mut self) -> &mut Vec<Num> {
        self.values.clear();
        &mut self.values
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::stats::unit_state::UnitState;
    use crate::stats::unit_states::UnitStates;
    use crate::stats::unit_stats::UnitStats;

    impl UnitStats {
        /// Stats of no value in `states`, as a unit's modifiers would put it in them.
        pub(crate) fn in_states(states: &[UnitState]) -> UnitStats {
            UnitStats {
                values: Vec::new(),
                states: UnitStates::of(states.iter().copied()),
            }
        }
    }
}
