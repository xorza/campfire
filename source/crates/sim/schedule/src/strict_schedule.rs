use bevy_ecs::schedule::{LogLevel, Schedule, ScheduleBuildSettings, ScheduleLabel};

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
mod tests {
    use bevy_ecs::schedule::ScheduleLabel;

    use super::*;

    #[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct Probe;

    #[test]
    fn a_strict_schedule_fails_on_ambiguity_and_redundant_edges_and_keeps_its_other_settings() {
        let mut schedule = StrictSchedule::of(Probe);
        let strict = schedule.get_build_settings();
        assert_eq!(strict.ambiguity_detection, LogLevel::Error);
        assert_eq!(strict.hierarchy_detection, LogLevel::Error);
        assert!(strict.auto_insert_apply_deferred);
        StrictSchedule::without_sync_points(&mut schedule);
        let probed = schedule.get_build_settings();
        assert!(!probed.auto_insert_apply_deferred);
        assert_eq!(probed.ambiguity_detection, LogLevel::Error);
        assert_eq!(probed.hierarchy_detection, LogLevel::Error);
        // A schedule another plugin made becomes strict and keeps its other settings.
        let mut made = Schedule::new(Probe);
        made.set_build_settings(ScheduleBuildSettings {
            auto_insert_apply_deferred: false,
            ..ScheduleBuildSettings::new()
        });
        StrictSchedule::apply(&mut made);
        let applied = made.get_build_settings();
        assert_eq!(applied.ambiguity_detection, LogLevel::Error);
        assert_eq!(applied.hierarchy_detection, LogLevel::Error);
        assert!(!applied.auto_insert_apply_deferred);
    }
}
