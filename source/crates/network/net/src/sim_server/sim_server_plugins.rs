use std::num::NonZeroU32;

use bevy_app::{App, FixedUpdate, Plugin, PluginGroup, PluginGroupBuilder, TaskPoolPlugin, Update};
use bevy_ecs::schedule::{ScheduleLabel, Schedules, SingleThreadedExecutor};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_declare::CapabilitySet;
use campfire_schedule::StrictSchedule;
use campfire_sim::MatchRate;

use crate::hosting::Hosting;
use crate::sim_server::SimServer;

/// The plugins of every server's app, as Bevy's `MinimalPlugins` groups its own: the task pool,
/// time and states, and `SimServer` at the session's rate `tick_hz`, whose components of the mode's
/// `capabilities` alone replicate, hosted as `hosting` says, every schedule on one thread. Its app
/// adds its run loop, where it runs one, its session and its transport.
#[derive(Debug)]
pub struct SimServerPlugins {
    pub tick_hz: NonZeroU32,
    pub capabilities: CapabilitySet,
    pub hosting: Hosting,
}

impl PluginGroup for SimServerPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(TaskPoolPlugin::default())
            .add(TimePlugin)
            .add(StatesPlugin)
            .add(SimServer {
                rate: MatchRate::new(self.tick_hz).rate(),
                capabilities: self.capabilities,
                hosting: self.hosting,
            })
            .add(OneThread)
    }
}

/// Runs every schedule of the server's app on Bevy's single-threaded executor, which it sets as
/// the app finishes, after every plugin added its schedules; the sim's tick, added as a match
/// starts, sets its own. A build turns Bevy's `multi_threaded` on wherever one of its members
/// enables it: without this, a server built beside the client runs its systems in parallel, and
/// one built alone does not, and systems with no order between them run in an order that timing
/// sets (02-engine-core.md, Bevy). The schedules of the server's frame, `Update` and
/// `FixedUpdate`, are strict, so the executor's own order decides nothing there.
#[derive(Debug)]
struct OneThread;

impl Plugin for OneThread {
    fn build(&self, _: &mut App) {}

    fn cleanup(&self, app: &mut App) {
        let mut schedules = app.world_mut().resource_mut::<Schedules>();
        let frame = [Update.intern(), FixedUpdate.intern()];
        for (_, schedule) in schedules.iter_mut() {
            schedule.set_executor(SingleThreadedExecutor::new());
            if frame.contains(&schedule.label()) {
                StrictSchedule::apply(schedule);
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
            .add_systems(Update, record.after(ServerFrameSystems::Saves))
            .add_systems(PostUpdate, record)
            .add_systems(Last, record);
        app.finish();
        app.cleanup();
        app.update();
        let here = thread::current().id();
        assert_eq!(app.world().resource::<Ran>().0, vec![here; 5]);
    }
}
