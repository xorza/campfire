use std::thread::{self, ThreadId};

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::ResMut;
use bevy_ecs::world::World;

use super::*;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct First;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Second;

/// The thread each system ran on.
#[derive(Resource, Debug, Default)]
struct Ran(Vec<ThreadId>);

fn record(mut ran: ResMut<'_, Ran>) {
    ran.0.push(thread::current().id());
}

/// Each schedule runs its systems on the thread that runs it.
#[test]
fn each_schedule_runs_on_the_thread_that_runs_it() {
    let mut world = World::new();
    world.init_resource::<Ran>();
    let mut schedules = Schedules::new();
    for label in [First.intern(), Second.intern()] {
        let mut schedule = Schedule::new(label);
        schedule.add_systems((record, record));
        schedules.insert(schedule);
    }
    OneThreadSchedule::apply_all(&mut schedules);
    for (_, schedule) in schedules.iter_mut() {
        schedule.run(&mut world);
    }
    let here = thread::current().id();
    assert_eq!(world.resource::<Ran>().0, [here; 4]);
}
