use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::units::unit_state::UnitState;
use crate::units::units_call::StateWrite;

/// The script state that calls wrote to units they created which spawn later in the tick, as
/// deliveries do: each write, in the order the calls made them. Not state: a unit takes its
/// writes as it spawns within the tick, and the writes to one that never spawned clear as the
/// next tick begins.
#[derive(Resource, Debug, Default)]
pub(crate) struct NewUnitStates(Vec<StateWrite>);

impl NewUnitStates {
    pub(crate) fn push(&mut self, write: StateWrite) {
        self.0.push(write);
    }

    /// Writes to `state` what the calls wrote to `unit`, in order, as it spawns.
    pub(crate) fn take(&mut self, unit: StableId, state: &mut UnitState) {
        for write in self.0.extract_if(.., |write| write.unit == unit) {
            state.set(write.at, write.value);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests;
