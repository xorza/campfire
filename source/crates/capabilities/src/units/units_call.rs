use bevy_ecs::world::World;
use campfire_sim::{SimWorld, StableId};

use crate::scripts::call_part::CallPart;
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::state_value::StateValue;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::unit_state_book::{StateField, StatePlace};
use crate::units::unit_states::UnitStates;
use crate::units::units_column::UnitsColumn;
use crate::units::view::View;

/// What the core adds to the call frame for units' script state: the writes the call made, in
/// order, which it reads back and which apply to the units when it ends.
#[derive(Debug, Default)]
pub(crate) struct UnitsCall {
    writes: Vec<StateWrite>,
}

/// A call's write of `value` to the field at `at` of unit `unit`, held at `place`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StateWrite {
    pub(crate) id: StableId,
    pub(crate) at: usize,
    pub(crate) place: StatePlace,
    pub(crate) value: StateValue,
}

impl CallPart for UnitsCall {
    fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
        self.writes.clear();
        Ok(())
    }

    fn param(&self, _: Option<ScriptRole>, _: &str) -> Option<ScriptValue> {
        None
    }

    /// Writes each value to its unit and to the view's row of it, in the order the call wrote
    /// them; a unit not spawned by then, as a delivery the call created, takes it as it spawns.
    fn apply(&mut self, world: &mut World) {
        let view = world.non_send::<View>().clone();
        for write in self.writes.drain(..) {
            if let Some(row) = view.row_index(write.id) {
                UnitsColumn::write(&view, row, write.at, write.value.clone());
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
    pub(crate) fn of(frame: &Frame) -> &UnitsCall {
        frame
            .part()
            .expect("a match that runs scripts has the core's part")
    }

    /// The core's part of `frame`, to change.
    pub(crate) fn of_mut(frame: &mut Frame) -> &mut UnitsCall {
        frame
            .part_mut()
            .expect("a match that runs scripts has the core's part")
    }

    /// The last value the call wrote at `at` of `id`, if it wrote one.
    pub(crate) fn written(&self, id: StableId, at: usize) -> Option<&StateValue> {
        let writes = self.writes.iter().rev();
        let mut found = writes.filter(|write| write.id == id && write.at == at);
        found.next().map(|write| &write.value)
    }

    /// Records the call's write of `value` to `field` of `id`.
    pub(crate) fn write(&mut self, id: StableId, field: StateField, value: StateValue) {
        self.writes.push(StateWrite {
            id,
            at: field.at,
            place: field.place,
            value,
        });
    }
}
