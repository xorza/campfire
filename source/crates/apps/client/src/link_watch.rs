use bevy::app::{App, AppExit, Plugin, Update};
use bevy::ecs::message::MessageWriter;
use bevy::ecs::system::Res;
use campfire_net::JoinState;

/// Ends the app with failure once the client stops: its link failed before it could take its
/// match up again, the server rewrote its chain, or it refused the session. A client whose link
/// fails while it plays tries again, which `SimClient` runs, and logs its loss when it stops.
#[derive(Debug)]
pub(crate) struct LinkWatch;

impl Plugin for LinkWatch {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, LinkWatch::stopped);
    }
}

impl LinkWatch {
    fn stopped(state: Res<'_, JoinState>, mut exit: MessageWriter<'_, AppExit>) {
        if state.loss().is_some() || state.refusal().is_some() {
            exit.write(AppExit::error());
        }
    }
}
