use bevy_ecs::schedule::{
    LogLevel, Schedule, ScheduleBuildSettings, ScheduleLabel, SingleThreadedExecutor,
};

/// The settings Campfire gives a schedule. A strict schedule's build fails on two systems with
/// conflicting access and no order, as either order could win, and on a set membership that a
/// longer path already implies, so a redundant edge shows in every build, not only as a log line.
/// A schedule on one thread runs its systems on the thread that runs it, in one order in every
/// run, with no task: Bevy's parallel executor, which its `multi_threaded` feature turns on
/// wherever a build's member enables it, runs them on its pool's threads in an order timing sets
/// (02-engine-core.md, The sim is one schedule on one thread).
pub trait ScheduleExt {
    /// A schedule of `label` with the strict build settings.
    fn strict(label: impl ScheduleLabel) -> Self;

    /// Gives the schedule, which another plugin may have made, the strict build settings, its
    /// automatic sync points kept.
    fn make_strict(&mut self);

    /// Runs the schedule's systems on the thread that runs it.
    fn run_on_one_thread(&mut self);
}

impl ScheduleExt for Schedule {
    fn strict(label: impl ScheduleLabel) -> Self {
        let mut schedule = Self::new(label);
        schedule.make_strict();
        schedule
    }

    fn make_strict(&mut self) {
        self.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            hierarchy_detection: LogLevel::Error,
            ..self.get_build_settings()
        });
    }

    fn run_on_one_thread(&mut self) {
        self.set_executor(SingleThreadedExecutor::new());
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::schedule::{Schedule, ScheduleBuildSettings};

    /// The settings only a test or a bench gives a schedule.
    pub trait ScheduleExtInternals {
        /// Takes the automatic sync points out, as a probe of a schedule's own order does.
        fn without_sync_points(&mut self);
    }

    impl ScheduleExtInternals for Schedule {
        fn without_sync_points(&mut self) {
            let settings = ScheduleBuildSettings {
                auto_insert_apply_deferred: false,
                ..self.get_build_settings()
            };
            self.set_build_settings(settings);
        }
    }
}

#[cfg(test)]
mod tests;
