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
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            hierarchy_detection: LogLevel::Error,
            ..ScheduleBuildSettings::new()
        });
        schedule
    }

    /// Turns off `schedule`'s automatic sync points, its other build settings kept: a sync point
    /// orders the systems it lies between, so an ambiguity check then sees every pair that only
    /// a sync point keeps in order.
    pub fn without_sync_points(schedule: &mut Schedule) {
        let settings = ScheduleBuildSettings {
            auto_insert_apply_deferred: false,
            ..schedule.get_build_settings()
        };
        schedule.set_build_settings(settings);
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::schedule::ScheduleLabel;

    use super::*;

    #[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct Probe;

    #[test]
    fn a_strict_schedule_fails_on_ambiguity_and_redundant_edges_and_drops_only_its_sync_points() {
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
    }
}
