use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::units::unit_states::UnitStates;
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

    /// Writes to `states` what the calls wrote to `id`, in order, as it spawns.
    pub(crate) fn take(&mut self, id: StableId, states: &mut UnitStates) {
        for write in self.0.extract_if(.., |write| write.id == id) {
            states.set(write.place, write.value);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests;
