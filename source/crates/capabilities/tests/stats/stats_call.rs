use std::ptr;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_common::Tick;
use campfire_script::{CallPart, CallStart, ScriptRole, StateValue, View};
use campfire_sim::IdAllocator;
use campfire_stats::*;
use campfire_values::ModifierId;

#[test]
fn a_handle_no_script_holds_serves_the_next_call_again() {
    // No world holds units for the view to read.
    let view = View::new(Match::TEST_RATE, |_| {});
    let carrier = IdAllocator::default().allocate();
    let of = |stacks| HandleOf {
        carrier,
        id: ModifierId::new(0),
        source: None,
        stacks,
    };
    let handle = |stacks, state: &[StateValue]| {
        ModifierHandle::new(None, of(stacks), state, Arc::default(), view.clone())
    };
    // The call holds two handles; a script still holds a copy of the second as the next
    // call begins, so only the first is spare.
    let mut call = StatsCall::default();
    call.handles_mut().push(handle(1, &[StateValue::Int(1)]));
    call.handles_mut().push(handle(2, &[]));
    let held = call.handles_mut()[1].clone();
    let first = ptr::from_ref(&*call.handles_mut()[0].data()).addr();
    let mut world = World::new();
    world.init_resource::<ParamBook>();
    call.begin(&world, &CallStart::mode(ScriptRole::Mode, Tick::ZERO))
        .unwrap();
    assert!(call.handles_mut().is_empty());
    let spare = call.spare().unwrap();
    assert!(call.spare().is_none());
    // Filled again, it is the first's storage with the new instance, and nothing written.
    let state = [StateValue::Int(7), StateValue::Int(8)];
    let again = ModifierHandle::new(Some(spare), of(3), &state, Arc::default(), view.clone());
    let data = again.data();
    assert_eq!(ptr::from_ref(&*data).addr(), first);
    assert_eq!((data.stacks, data.state.as_slice()), (3, &state[..]));
    assert!(!data.written && !data.removed);
    assert_eq!(held.data().stacks, 2);
}
