use std::thread::{self, ThreadId};

use bevy_app::{First, Last, PostUpdate, PreUpdate};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::ResMut;

use super::*;
use crate::sim_server::ServerFrameSystems;

/// The thread each system of the test ran on.
#[derive(Resource, Debug, Default)]
struct Ran(Vec<ThreadId>);

fn record(mut ran: ResMut<'_, Ran>) {
    ran.0.push(thread::current().id());
}

#[test]
fn every_schedule_of_the_server_runs_on_the_thread_that_updates_it() {
    // Bevy's parallel executor runs each system it can send on its pool's threads, never on
    // the thread that updates the app, so a system of any schedule there would show another
    // thread, whichever features the test's build turns on.
    let mut app = App::new();
    app.add_plugins(SimServerPlugins {
        tick_hz: NonZeroU32::new(30).unwrap(),
        capabilities: CapabilitySet::CORE,
        hosting: Hosting::Remote,
    });
    app.init_resource::<Ran>();
    app.add_systems(First, record)
        .add_systems(PreUpdate, record)
        .add_systems(Update, record.after(ServerFrameSystems::Console))
        .add_systems(PostUpdate, record)
        .add_systems(Last, record);
    app.finish();
    app.cleanup();
    app.update();
    let here = thread::current().id();
    assert_eq!(app.world().resource::<Ran>().0, vec![here; 5]);
}
