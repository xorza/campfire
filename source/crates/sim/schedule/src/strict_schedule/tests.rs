use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleLabel, SystemSet};
use bevy_ecs::world::World;

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
        StrictSchedule::apply_hierarchy_all(&mut schedules);
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
