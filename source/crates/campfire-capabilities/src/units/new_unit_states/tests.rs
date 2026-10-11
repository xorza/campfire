use campfire_sim::IdAllocator;

use super::*;
use crate::scripts::state_decl::synced_state_decl::SyncTo;
use crate::scripts::state_value::StateValue;
use crate::units::unit_state_book::StatePlace;

#[test]
fn a_spawn_takes_its_own_writes_in_order_and_a_clear_drops_the_rest() {
    let mut ids = IdAllocator::default();
    let (unit, other) = (ids.allocate(), ids.allocate());
    // Field 0 is its owner's first, field 1 its owner's second, field 2 the first of all's.
    let places = [(SyncTo::Owner, 0), (SyncTo::Owner, 1), (SyncTo::All, 0)];
    let write = |unit, at: usize, value| StateWrite {
        unit,
        at,
        place: StatePlace {
            sync: places[at].0,
            at: places[at].1,
        },
        value: StateValue::Int(value),
    };
    let mut states = NewUnitStates::default();
    states.push(write(unit, 0, 1));
    states.push(write(other, 0, 9));
    states.push(write(unit, 0, 2));
    states.push(write(unit, 1, 3));
    states.push(write(unit, 2, 4));
    let held = |values: [i64; 3]| {
        let mut held = UnitStates::default();
        for ((sync, _), value) in places.into_iter().zip(values) {
            held.push(sync, StateValue::Int(value));
        }
        held
    };
    // `unit` takes 1 and then 2 at field 0, 3 at field 1 and 4 at field 2, each at its place;
    // `other`'s write stays.
    let mut state = held([0; 3]);
    states.take(unit, &mut state);
    assert_eq!(state, held([2, 3, 4]));
    assert_eq!(states.0, [write(other, 0, 9)]);
    // Taken, `unit`'s writes are gone; cleared, so is `other`'s.
    let mut state = held([0; 3]);
    states.take(unit, &mut state);
    assert_eq!(state, held([0; 3]));
    states.clear();
    states.take(other, &mut state);
    assert_eq!(state, held([0; 3]));
}
