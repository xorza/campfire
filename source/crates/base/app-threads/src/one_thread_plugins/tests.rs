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
