use bevy_app::AppExit;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Res, ResMut, Single};
use campfire_common::Tick;
use campfire_net_connection::{Client, Disconnected};
use campfire_net_sync::SyncedLocalTimeline;
use tracing::info;

use crate::sim_client::LeaveRequest;
use campfire_host::JoinState;

/// The leave of a client a bot script plays, after the tick the script ends at: whether it asked
/// to leave yet.
#[derive(Resource, Debug)]
pub(crate) struct BotLeave {
    after: Tick,
    asked: bool,
}

impl BotLeave {
    pub(crate) const fn new(after: Tick) -> Self {
        Self {
            after,
            asked: false,
        }
    }

    /// Once the client ran the end tick, tells the server the player leaves; once the server
    /// ended the link, ends the app.
    pub(crate) fn leave(
        timeline: Option<SyncedLocalTimeline<'_, '_>>,
        state: Res<'_, JoinState>,
        client: Single<'_, '_, Option<&Disconnected>, With<Client>>,
        mut leave: ResMut<'_, Self>,
        mut commands: Commands<'_, '_>,
        mut exit: MessageWriter<'_, AppExit>,
    ) {
        let disconnected = *client;
        if leave.asked {
            if disconnected.is_some() {
                exit.write(AppExit::Success);
            }
            return;
        }
        let Some(tick) = state.sim_tick(timeline.as_ref().map(|timeline| timeline.tick())) else {
            return;
        };
        if tick > leave.after {
            info!(tick = tick.get(), "the script ended; leaving the match");
            commands.insert_resource(LeaveRequest);
            leave.asked = true;
        }
    }
}
