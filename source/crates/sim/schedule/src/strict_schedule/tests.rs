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
