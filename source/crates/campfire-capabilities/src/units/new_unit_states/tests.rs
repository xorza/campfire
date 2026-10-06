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
