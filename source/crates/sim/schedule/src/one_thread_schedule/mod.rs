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
mod tests;
