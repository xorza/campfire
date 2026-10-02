use bevy::app::{App, AppExit, Plugin};
use bevy::ecs::lifecycle::Insert;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res};
use campfire_log::LogEvent;
use campfire_net::{JoinState, LinkLost};
use lightyear::prelude::{Client, UnlinkReason, Unlinked};

/// Ends the app with failure when the link to the server fails or ends before the client asked
/// to leave, or the client refused the session: a client with no link has nothing left to do.
#[derive(Debug)]
pub(crate) struct LinkWatch;

impl Plugin for LinkWatch {
    fn build(&self, app: &mut App) {
        app.add_observer(LinkWatch::lost);
    }
}

impl LinkWatch {
    /// A client starts unlinked, before it links, and unlinks when it asks to leave, or when it
    /// refuses the session, which it logged; any other reason came from outside.
    fn lost(
        unlinked: On<'_, '_, Insert, Unlinked>,
        clients: Query<'_, '_, &Unlinked, With<Client>>,
        state: Res<'_, JoinState>,
        mut exit: MessageWriter<'_, AppExit>,
    ) {
        let Ok(Unlinked { reason }) = clients.get(unlinked.entity) else {
            return;
        };
        match reason {
            UnlinkReason::Initial => return,
            UnlinkReason::UserRequested(_) if state.refusal().is_none() => return,
            UnlinkReason::UserRequested(_) => {}
            _ => LinkLost {
                reason: reason.to_string(),
            }
            .log(),
        }
        exit.write(AppExit::error());
    }
}
