use bevy_app::{App, Plugin, PluginGroup, PluginGroupBuilder, TaskPoolOptions, TaskPoolPlugin};
use bevy_ecs::schedule::Schedules;
use bevy_tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool, TaskPool, block_on};
use campfire_schedule::SchedulesExt;

/// The plugins of an app of one thread, a server's, a bot's client's or a test's: Bevy's task
/// pools of one thread each, and each schedule of the app on the thread that updates it, set as
/// the app finishes, after every plugin added its schedules; code that adds a schedule after
/// that, as a test's system of a new label, gives it `SchedulesExt::run_on_one_thread` itself.
/// The pools are the process's, which the first app of a process sizes, so an app built after
/// another runs on that app's (02-engine-core.md, An app's threads are one plan; 02-engine-core.md, Pools belong to the
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
        app.world_mut()
            .resource_mut::<Schedules>()
            .run_on_one_thread();
    }
}

#[cfg(test)]
mod tests;
