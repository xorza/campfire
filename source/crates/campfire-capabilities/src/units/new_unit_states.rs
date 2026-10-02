use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::units::unit_state::UnitState;
use crate::units::unit_state_call::StateWrite;

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
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;
    use crate::scripts::state_value::StateValue;

    #[test]
    fn a_spawn_takes_its_own_writes_in_order_and_a_clear_drops_the_rest() {
        let mut ids = IdAllocator::default();
        let (unit, other) = (ids.allocate(), ids.allocate());
        let write = |unit, at, value| StateWrite {
            unit,
            at,
            value: StateValue::Int(value),
        };
        let mut states = NewUnitStates::default();
        states.push(write(unit, 0, 1));
        states.push(write(other, 0, 9));
        states.push(write(unit, 0, 2));
        states.push(write(unit, 1, 3));
        // `unit` takes 1 and then 2 at field 0, and 3 at field 1; `other`'s write stays.
        let zeros = || UnitState::new(vec![StateValue::Int(0), StateValue::Int(0)]);
        let mut state = zeros();
        states.take(unit, &mut state);
        assert_eq!(state.values(), [StateValue::Int(2), StateValue::Int(3)]);
        assert_eq!(states.0, [write(other, 0, 9)]);
        // Taken, `unit`'s writes are gone; cleared, so is `other`'s.
        let mut state = zeros();
        states.take(unit, &mut state);
        assert_eq!(state, zeros());
        states.clear();
        states.take(other, &mut state);
        assert_eq!(state, zeros());
    }
}
