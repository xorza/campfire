use bevy_ecs::schedule::{Schedule, Schedules, SingleThreadedExecutor};

/// A schedule whose systems run on the thread that runs it, in one order in every run, with no
/// task: Bevy's parallel executor, which its `multi_threaded` feature turns on wherever a build's
/// member enables it, runs them on its pool's threads in an order timing sets (02-engine-core.md,
/// The sim is one schedule on one thread).
#[derive(Debug)]
pub struct OneThreadSchedule;

impl OneThreadSchedule {
    pub fn apply(schedule: &mut Schedule) {
        schedule.set_executor(SingleThreadedExecutor::new());
    }

    /// Each schedule of `schedules`.
    pub fn apply_all(schedules: &mut Schedules) {
        for (_, schedule) in schedules.iter_mut() {
            Self::apply(schedule);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
