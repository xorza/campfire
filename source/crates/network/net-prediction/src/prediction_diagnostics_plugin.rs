use core::time::Duration;

use bevy_app::{App, Plugin, PostUpdate};
use bevy_diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Local, Res};
use bevy_time::common_conditions::on_timer;

use crate::prediction_counts::PredictionCounts;

/// Plugin in charge of collecting diagnostics for the prediction systems.
#[derive(Debug)]
pub struct PredictionDiagnosticsPlugin {
    /// Number of diagnostics to keep in history
    history_length: usize,
    /// How often to flush the stored data into the Diagnostics
    flush_interval: Duration,
}

impl Default for PredictionDiagnosticsPlugin {
    fn default() -> Self {
        Self {
            history_length: 60,
            flush_interval: Duration::from_millis(200),
        }
    }
}

/// The metric totals at the last flush.
#[derive(Debug, Default)]
struct Flushed {
    rollbacks: u32,
    rollback_ticks: u32,
}

impl PredictionDiagnosticsPlugin {
    /// Number of rollbacks in a flush interval
    pub const ROLLBACKS: DiagnosticPath =
        DiagnosticPath::const_new("replication.prediction.rollbacks");

    /// Number of ticks resimulated as part of rollbacks in a flush interval
    pub const ROLLBACK_TICKS: DiagnosticPath =
        DiagnosticPath::const_new("replication.prediction.rollback_ticks");

    /// Average rollback depth in a flush interval
    pub const ROLLBACK_DEPTH: DiagnosticPath =
        DiagnosticPath::const_new("replication.prediction.rollback_depth");

    /// Measures the rollbacks since the last flush, so that each measurement and the depth's
    /// average are the interval's, while the counts keep their totals.
    fn flush_measurements(
        counts: Res<'_, PredictionCounts>,
        mut flushed: Local<'_, Flushed>,
        mut diagnostics: Diagnostics<'_, '_>,
    ) {
        let rollbacks = counts.rollbacks.saturating_sub(flushed.rollbacks);
        let rollback_ticks = counts.rollback_ticks.saturating_sub(flushed.rollback_ticks);
        *flushed = Flushed {
            rollbacks: counts.rollbacks,
            rollback_ticks: counts.rollback_ticks,
        };
        diagnostics.add_measurement(&Self::ROLLBACKS, || f64::from(rollbacks));
        diagnostics.add_measurement(&Self::ROLLBACK_TICKS, || f64::from(rollback_ticks));
        diagnostics.add_measurement(&Self::ROLLBACK_DEPTH, || {
            if rollbacks == 0 {
                0.0
            } else {
                f64::from(rollback_ticks) / f64::from(rollbacks)
            }
        });
    }
}

impl Plugin for PredictionDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        let should_run = on_timer(self.flush_interval);

        app.add_systems(PostUpdate, Self::flush_measurements.run_if(should_run));
        for (path, suffix) in [
            (Self::ROLLBACKS, "rollbacks"),
            (Self::ROLLBACK_TICKS, "ticks resimulated during rollback"),
            (Self::ROLLBACK_DEPTH, "Average rollback depth"),
        ] {
            // `with_max_history_length` reserves the length less the capacity it finds, so from
            // `Diagnostic::new`'s 20 it reserves 20 short and the history grows on a later frame.
            // From a capacity of zero it reserves the whole length.
            app.register_diagnostic(
                Diagnostic::new(path)
                    .with_suffix(suffix)
                    .with_max_history_length(0)
                    .with_max_history_length(self.history_length),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_diagnostic::DiagnosticsStore;
    use bevy_ecs::world::World;

    fn latest(world: &World, path: &DiagnosticPath) -> Option<f64> {
        world
            .resource::<DiagnosticsStore>()
            .get(path)?
            .measurement()
            .map(|m| m.value)
    }

    /// Each flush measures the interval since the last: 3 rollbacks of 12 ticks, then 1 more of
    /// 2 ticks, read 3, 12, 4 and then 1, 2, 2.
    #[test]
    fn each_flush_measures_its_interval() {
        let mut app = App::new();
        app.add_plugins(PredictionDiagnosticsPlugin::default());
        app.init_resource::<DiagnosticsStore>();
        // The rollbacks' counts, which the rollback plugin makes and writes.
        app.init_resource::<PredictionCounts>();
        app.finish();
        let world = app.world_mut();
        // A registered system keeps its `Local` across runs, as a scheduled one does.
        let system = world.register_system(PredictionDiagnosticsPlugin::flush_measurements);
        let flush = |world: &mut World, rollbacks, rollback_ticks| {
            *world.resource_mut::<PredictionCounts>() = PredictionCounts {
                rollbacks,
                rollback_ticks,
            };
            world.run_system(system).unwrap();
            [
                PredictionDiagnosticsPlugin::ROLLBACKS,
                PredictionDiagnosticsPlugin::ROLLBACK_TICKS,
                PredictionDiagnosticsPlugin::ROLLBACK_DEPTH,
            ]
            .map(|path| latest(world, &path))
        };

        assert_eq!(flush(world, 3, 12), [Some(3.0), Some(12.0), Some(4.0)]);
        assert_eq!(flush(world, 4, 14), [Some(1.0), Some(2.0), Some(2.0)]);
    }
}
