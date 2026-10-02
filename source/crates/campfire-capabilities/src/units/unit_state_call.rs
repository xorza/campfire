use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;
use campfire_sim::{EntityIndex, StableId};

use crate::scripts::call_part::CallPart;
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::state_value::StateValue;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::script_view::View;
use crate::units::unit_state::UnitState;
use crate::units::unit_state_column::UnitStateColumn;

/// What the core adds to the call frame for units' script state: the writes the call made, in
/// order, which it reads back and which apply to the units when it ends.
#[derive(Debug, Default)]
pub(crate) struct UnitStateCall {
    writes: Vec<StateWrite>,
}

/// A call's write of `value` to the field at `at` of unit `unit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StateWrite {
    pub(crate) unit: StableId,
    pub(crate) at: usize,
    pub(crate) value: StateValue,
}

impl CallPart for UnitStateCall {
    fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
        self.writes.clear();
        Ok(())
    }

    fn param_named(&self, _: Option<ScriptRole>, _: &str) -> Option<Dynamic> {
        None
    }

    /// Writes each value to its unit and to the view's row of it, in the order the call wrote
    /// them; a unit not spawned by then, as a delivery the call created, takes it as it spawns.
    fn apply(&mut self, world: &mut World) {
        let view = world.non_send::<View>().clone();
        for write in self.writes.drain(..) {
            if let Some(row) = view.row_index(write.unit) {
                UnitStateColumn::write(&view, row, write.at, write.value.clone());
            }
            let entity = world.resource::<EntityIndex>().get(write.unit);
            match entity.and_then(|entity| world.get_mut::<UnitState>(entity)) {
                Some(mut state) => state.set(write.at, write.value),
                None => world.resource_mut::<NewUnitStates>().push(write),
            }
        }
    }
}

impl UnitStateCall {
    /// The core's part of `frame`; the core adds it as it installs a match that runs scripts.
    pub(crate) fn of(frame: &Frame) -> &UnitStateCall {
        frame
            .part()
            .expect("a match that runs scripts has the core's part")
    }

    /// The core's part of `frame`, to change.
    pub(crate) fn of_mut(frame: &mut Frame) -> &mut UnitStateCall {
        frame
            .part_mut()
            .expect("a match that runs scripts has the core's part")
    }

    /// The last value the call wrote at `at` of `unit`, if it wrote one.
    pub(crate) fn written(&self, unit: StableId, at: usize) -> Option<&StateValue> {
        let writes = self.writes.iter().rev();
        let mut found = writes.filter(|write| write.unit == unit && write.at == at);
        found.next().map(|write| &write.value)
    }

    /// Records the call's write of `value` at `at` of `unit`.
    pub(crate) fn write(&mut self, unit: StableId, at: usize, value: StateValue) {
        self.writes.push(StateWrite { unit, at, value });
    }
}
