use bevy_ecs::world::World;
use campfire_script::{
    CallError, CallPart, CallStart, Commit, Frame, StateField, StateValue, View,
};
use campfire_sim::{SimWorld, StableId};

use crate::units::new_unit_states::NewUnitStates;
use crate::units::state_write::StateWrite;
use crate::units::unit_states::UnitStates;
use crate::units::units_column::UnitsColumn;

/// What the core adds to the call frame for units' script state: the writes the call made, in
/// order, which it reads back and which apply to the units when it ends.
#[derive(Debug, Default)]
pub struct UnitsCall {
    writes: Vec<StateWrite>,
}

impl CallPart for UnitsCall {
    /// After the effects, so a unit an effect spawns takes the call's writes to it.
    const COMMITS: Commit = Commit::AfterEffects;

    fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
        self.writes.clear();
        Ok(())
    }

    /// Writes each value to its unit and to the view's row of it, in the order the call wrote
    /// them; a unit not spawned by then, as a delivery the call created, takes it as it spawns.
    fn commit(&mut self, world: &mut World) {
        let view = world.non_send::<View>().clone();
        for write in self.writes.drain(..) {
            if let Some(row) = view.row_index(write.id) {
                UnitsColumn::write(&view, row, write.field, write.value.clone());
            }
            match world.entity_of(write.id) {
                Some(entity) => UnitStates::write(world, entity, write.place, write.value),
                None => world.resource_mut::<NewUnitStates>().push(write),
            }
        }
    }
}

impl UnitsCall {
    /// The core's part of `frame`; the core adds it as it installs a match that runs scripts.
    pub fn of(frame: &Frame) -> &UnitsCall {
        frame
            .part()
            .expect("a match that runs scripts has the core's part")
    }

    /// The core's part of `frame`, to change.
    pub fn of_mut(frame: &mut Frame) -> &mut UnitsCall {
        frame
            .part_mut()
            .expect("a match that runs scripts has the core's part")
    }

    /// The last value the call wrote to `field` of `id`, if it wrote one.
    pub(crate) fn written(&self, id: StableId, field: StateField) -> Option<&StateValue> {
        let writes = self.writes.iter().rev();
        let mut found = writes.filter(|write| write.id == id && write.field == field);
        found.next().map(|write| &write.value)
    }

    /// Records the call's write, which it reads back and which commits as it ends.
    pub(crate) fn write(&mut self, write: StateWrite) {
        self.writes.push(write);
    }
}
