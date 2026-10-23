use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings, Schedules};

use crate::schedule_ext::ScheduleExt;

/// The settings Campfire gives every schedule of an app, as `ScheduleExt` gives one.
pub trait SchedulesExt {
    /// Gives each schedule the hierarchy check alone, its other build settings kept: a set
    /// membership that a longer path already implies fails its build, which no order another
    /// plugin's systems leave open can cause, as an ambiguity can.
    fn make_hierarchy_strict(&mut self);

    /// Runs each schedule's systems on the thread that runs it.
    fn run_on_one_thread(&mut self);
}

impl SchedulesExt for Schedules {
    fn make_hierarchy_strict(&mut self) {
        for (_, schedule) in self.iter_mut() {
            schedule.set_build_settings(ScheduleBuildSettings {
                hierarchy_detection: LogLevel::Error,
                ..schedule.get_build_settings()
            });
        }
    }

    fn run_on_one_thread(&mut self) {
        for (_, schedule) in self.iter_mut() {
            schedule.run_on_one_thread();
        }
    }
}

#[cfg(test)]
mod tests;
