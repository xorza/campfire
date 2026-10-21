use bevy_ecs::schedule::{LogLevel, Schedule, ScheduleBuildSettings, ScheduleLabel, Schedules};

/// A schedule whose build fails on two systems with conflicting access and no order, as either
/// order could win, and on a set membership that a longer path already implies, so a redundant
/// edge shows in every build, not only as a log line.
#[derive(Debug)]
pub struct StrictSchedule;

impl StrictSchedule {
    /// A schedule of `label` with the strict build settings.
    pub fn of(label: impl ScheduleLabel) -> Schedule {
        let mut schedule = Schedule::new(label);
        Self::apply(&mut schedule);
        schedule
    }

    /// Gives `schedule`, which another plugin may have made, the strict build settings, its
    /// automatic sync points kept.
    pub fn apply(schedule: &mut Schedule) {
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            hierarchy_detection: LogLevel::Error,
            ..schedule.get_build_settings()
        });
    }

    /// Gives each schedule of `schedules` the hierarchy check alone, its other build settings
    /// kept: a set membership that a longer path already implies fails its build, which no order
    /// another plugin's systems leave open can cause, as an ambiguity can.
    pub fn apply_hierarchy_all(schedules: &mut Schedules) {
        for (_, schedule) in schedules.iter_mut() {
            schedule.set_build_settings(ScheduleBuildSettings {
                hierarchy_detection: LogLevel::Error,
                ..schedule.get_build_settings()
            });
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::schedule::{Schedule, ScheduleBuildSettings};

    use crate::strict_schedule::StrictSchedule;

    impl StrictSchedule {
        /// Turns off `schedule`'s automatic sync points, its other build settings kept: a sync
        /// point orders the systems it lies between, so an ambiguity check then sees every pair
        /// that only a sync point keeps in order.
        pub fn without_sync_points(schedule: &mut Schedule) {
            let settings = ScheduleBuildSettings {
                auto_insert_apply_deferred: false,
                ..schedule.get_build_settings()
            };
            schedule.set_build_settings(settings);
        }
    }
}

#[cfg(test)]
mod tests;
