use bevy_app::{App, Plugin, PluginGroup, PluginGroupBuilder, TaskPoolOptions, TaskPoolPlugin};
use bevy_ecs::schedule::Schedules;
use bevy_tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool, TaskPool, block_on};
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

/// Runs each schedule of the app on its single-threaded executor, and holds the app's finish
/// until each pool's thread runs.
#[derive(Debug)]
struct OneThreadExecutorPlugin;

impl Plugin for OneThreadExecutorPlugin {
    fn build(&self, _: &mut App) {}

    /// Waits until each pool's thread runs a task, as a thread allocates as it starts, before its
    /// first task, and the OS starts it when it will (02-engine-core.md, An app's threads are one
    /// plan).
    fn finish(&self, _: &mut App) {
        let pools: [&TaskPool; 3] = [
            ComputeTaskPool::get(),
            AsyncComputeTaskPool::get(),
            IoTaskPool::get(),
        ];
        for started in pools.map(|pool| pool.spawn(async {})) {
            block_on(started);
        }
    }

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
    /// test's build turns on, and each of Bevy's pools holds one thread of its own, which runs
    /// its tasks.
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
        let threads = [
            ComputeTaskPool::get().thread_num(),
            AsyncComputeTaskPool::get().thread_num(),
            IoTaskPool::get().thread_num(),
        ];
        assert_eq!(threads, [1; 3]);
        let pools: [&TaskPool; 3] = [
            ComputeTaskPool::get(),
            AsyncComputeTaskPool::get(),
            IoTaskPool::get(),
        ];
        for pool in pools {
            let ran = block_on(pool.spawn(async { thread::current().id() }));
            assert_ne!(ran, here);
        }
    }
}
