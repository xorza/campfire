use bevy_ecs::resource::Resource;
use campfire_declare::{Role, Scratch};
use campfire_sim::StableId;

use crate::state_write::StateWrite;
use crate::unit_states::UnitStates;

/// The script state that calls wrote to units they created which spawn later in the tick, as
/// deliveries do: each write, in the order the calls made them. Not state: a unit takes its
/// writes as it spawns within the tick, and the writes to one that never spawned clear as the
/// tick ends.
#[derive(Resource, Debug, Default)]
pub struct NewUnitStates(Vec<StateWrite>);

impl NewUnitStates {
    pub fn push(&mut self, write: StateWrite) {
        self.0.push(write);
    }

    /// Writes to `states` what the calls wrote to `id`, in order, as it spawns.
    pub fn take(&mut self, id: StableId, states: &mut UnitStates) {
        for write in self.0.extract_if(.., |write| write.id == id) {
            states.set(write.place, write.value);
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl Scratch for NewUnitStates {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests;
