use bevy::app::{App, AppExit, Plugin, Update};
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Res, ResMut, Single};
use campfire_common::Tick;
use campfire_net::{BotScript, JoinState, LeaveRequest, OrderScript};
use lightyear::prelude::{Client, Disconnected, SyncedLocalTimeline};
use tracing::info;

/// Plays an order script for the player, and leaves after the script's end tick when it names
/// one.
#[derive(Debug)]
pub(crate) struct Bot {
    pub(crate) script: OrderScript,
}

/// The tick after which the bot leaves, and whether it asked to disconnect yet.
#[derive(Resource, Debug)]
struct Leave {
    after: Tick,
    asked: bool,
}

impl Plugin for Bot {
    fn build(&self, app: &mut App) {
        app.insert_resource(BotScript::new(self.script.clone()));
        if let Some(after) = self.script.end() {
            app.insert_resource(Leave {
                after,
                asked: false,
            });
            app.add_systems(Update, Bot::leave);
        }
    }
}

impl Bot {
    /// Once the client ran the end tick, tells the server the player leaves; once the server
    /// ended the link, ends the app.
    fn leave(
        timeline: Option<SyncedLocalTimeline<'_, '_>>,
        state: Res<'_, JoinState>,
        client: Single<'_, '_, Option<&Disconnected>, With<Client>>,
        mut leave: ResMut<'_, Leave>,
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
        let Some(tick) = state.sim_tick(timeline.as_ref()) else {
            return;
        };
        if tick > leave.after {
            info!(tick = tick.get(), "the script ended; leaving the match");
            commands.insert_resource(LeaveRequest);
            leave.asked = true;
        }
    }
}
