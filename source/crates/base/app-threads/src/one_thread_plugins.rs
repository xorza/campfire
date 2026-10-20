use bevy_app::{App, Plugin, PluginGroup, PluginGroupBuilder, TaskPoolOptions, TaskPoolPlugin};
use bevy_ecs::schedule::Schedules;
use campfire_schedule::OneThreadSchedule;

/// The plugins of an app of one thread, a server's, a bot's client's or a test's: Bevy's task
/// pools of one thread each, and each schedule of the app on the thread that updates it, set as
/// the app finishes, after every plugin added its schedules; code that adds a schedule after
/// that, as a test's system of a new label, gives it `OneThreadSchedule` itself. The pools are
/// the process's, which
/// the first app of a process sizes, so an app built after another runs on that app's
/// (02-engine-core.md, An app's threads are one plan; 02-engine-core.md, Pools belong to the
/// process).
#[derive(Debug)]
pub struct OneThreadPlugins;

impl PluginGroup for OneThreadPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(TaskPoolPlugin {
                task_pool_options: TaskPoolOptions::with_num_threads(1),
            })
            .add(OneThreadExecutorPlugin)
    }
}

/// Runs each schedule of the app on its single-threaded executor.
#[derive(Debug)]
struct OneThreadExecutorPlugin;

impl Plugin for OneThreadExecutorPlugin {
    fn build(&self, _: &mut App) {}

    fn cleanup(&self, app: &mut App) {
        OneThreadSchedule::apply_all(&mut app.world_mut().resource_mut::<Schedules>());
    }
}

#[cfg(test)]
mod tests {
    use std::thread::{self, ThreadId};

    use bevy_app::{First, Last, PostUpdate, PreUpdate, Update};
    use bevy_ecs::resource::Resource;
    use bevy_ecs::schedule::ScheduleLabel;
    use bevy_ecs::system::ResMut;

    use super::*;

    /// The thread each system ran on.
    #[derive(Resource, Debug, Default)]
    struct Ran(Vec<ThreadId>);

    fn record(mut ran: ResMut<'_, Ran>) {
        ran.0.push(thread::current().id());
    }

    /// Each schedule of the app runs on the thread that updates it, whichever features the
    /// test's build turns on.
    #[test]
    fn an_app_of_one_thread_runs_every_schedule_on_its_thread() {
        let mut app = App::new();
        app.add_plugins(OneThreadPlugins);
        app.init_resource::<Ran>();
        for label in [First.intern(), PreUpdate.intern(), Update.intern()] {
            app.add_systems(label, (record, record));
        }
        app.add_systems(PostUpdate, record)
            .add_systems(Last, record);
        // As an app's runner does before its first update.
        app.finish();
        app.cleanup();
        app.update();
        let here = thread::current().id();
        assert_eq!(app.world().resource::<Ran>().0, [here; 8]);
    }
}
