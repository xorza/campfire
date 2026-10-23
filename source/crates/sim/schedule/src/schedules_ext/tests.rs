use std::thread::{self, ThreadId};

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, SystemSet};
use bevy_ecs::system::ResMut;
use bevy_ecs::world::World;

use super::*;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct First;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Probe;

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
    schedules.run_on_one_thread();
    for (_, schedule) in schedules.iter_mut() {
        schedule.run(&mut world);
    }
    let here = thread::current().id();
    assert_eq!(world.resource::<Ran>().0, [here; 4]);
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ProbeSystems {
    Outer,
    Inner,
}

fn probe() {}

/// The hierarchy check alone fails a system that joins a set and the set's parent, and leaves the
/// ambiguity check and the sync points as they were; a system in the inner set alone builds.
#[test]
fn the_hierarchy_check_alone_fails_a_redundant_edge() {
    let build = |redundant: bool| {
        let mut schedules = Schedules::new();
        let mut schedule = Schedule::new(Probe);
        schedule.configure_sets(ProbeSystems::Inner.in_set(ProbeSystems::Outer));
        if redundant {
            schedule.add_systems(
                probe
                    .in_set(ProbeSystems::Inner)
                    .in_set(ProbeSystems::Outer),
            );
        } else {
            schedule.add_systems(probe.in_set(ProbeSystems::Inner));
        }
        schedules.insert(schedule);
        schedules.make_hierarchy_strict();
        let mut schedule = schedules.remove(Probe).unwrap();
        let settings = schedule.get_build_settings();
        assert_eq!(settings.hierarchy_detection, LogLevel::Error);
        assert_eq!(settings.ambiguity_detection, LogLevel::Ignore);
        assert!(settings.auto_insert_apply_deferred);
        schedule.initialize(&mut World::new()).is_ok()
    };
    assert!(build(false));
    assert!(!build(true));
}
