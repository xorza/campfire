use std::sync::Mutex;

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::world::World;

/// What an app's next fixed tick runs once: a remote client's right after its `FixedUpdate`, put
/// there by `Stepper::client_fixed_update`, the server's right before, by
/// `Stepper::server_fixed_update`.
#[derive(Resource, Default)]
pub(crate) struct FixedRun(Mutex<Option<Run>>);

/// A closure a test hands a client's fixed tick.
type Run = Box<dyn FnOnce(&mut World) + Send>;

/// The schedule of a client's fixed tick that runs its `FixedRun`, right after `FixedUpdate`: a
/// schedule of its own, so a test's systems in the strict `FixedUpdate` need no order with it.
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AfterFixedUpdate;

/// The schedule of the server's fixed tick that runs its `FixedRun`, right before `FixedUpdate`,
/// whose systems of the tick read what it changed, as they read the sim's own changes.
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BeforeFixedUpdate;

impl FixedRun {
    pub(crate) fn set(&self, run: Run) {
        let previous = self.0.lock().unwrap().replace(run);
        debug_assert!(
            previous.is_none(),
            "a fixed run waits for one frame at most"
        );
    }

    /// Runs the closure `world`'s `FixedRun` holds, if any.
    pub(crate) fn run(world: &mut World) {
        let run = world.resource::<Self>().0.lock().unwrap().take();
        if let Some(run) = run {
            run(world);
        }
    }
}

impl std::fmt::Debug for FixedRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let waiting = self.0.lock().unwrap().is_some();
        f.debug_struct("FixedRun")
            .field("waiting", &waiting)
            .finish()
    }
}
