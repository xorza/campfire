use std::num::NonZeroU32;

use bevy_ecs::system::RunSystemOnce;
use bevy_ecs::world::World;

use super::*;

#[test]
fn a_clock_reads_the_tick_under_way_and_the_rate() {
    // Tick 5 runs from 5 to 6, at 30 a second.
    let mut world = World::new();
    world.insert_resource(SimTick::new(Tick::new(5)));
    world.insert_resource(MatchRate::new(NonZeroU32::new(30).unwrap()));
    let read = world.run_system_once(|clock: TickClock<'_>| {
        (clock.start(), clock.end(), clock.rate().hz().get())
    });
    assert_eq!(read.unwrap(), (Tick::new(5), Tick::new(6), 30));
}
