use bevy::app::{App, AppExit, Plugin};
use bevy::ecs::lifecycle::Insert;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::Query;
use campfire_log::LogEvent;
use campfire_net::LinkLost;
use lightyear::prelude::{Client, UnlinkReason, Unlinked};

/// Ends the app with failure when the link to the server fails or ends before the client asked
/// to leave: a client with no link has nothing left to do.
#[derive(Debug)]
pub(crate) struct LinkWatch;

impl Plugin for LinkWatch {
    fn build(&self, app: &mut App) {
        app.add_observer(LinkWatch::lost);
    }
}

impl LinkWatch {
    /// A client starts unlinked, before it links, and unlinks when it asks to leave; any other
    /// reason came from outside.
    fn lost(
        unlinked: On<'_, '_, Insert, Unlinked>,
        clients: Query<'_, '_, &Unlinked, With<Client>>,
        mut exit: MessageWriter<'_, AppExit>,
    ) {
        let Ok(Unlinked { reason }) = clients.get(unlinked.entity) else {
            return;
        };
        if matches!(
            reason,
            UnlinkReason::Initial | UnlinkReason::UserRequested(_)
        ) {
            return;
        }
        LinkLost {
            reason: reason.to_string(),
        }
        .log();
        exit.write(AppExit::error());
    }
}
