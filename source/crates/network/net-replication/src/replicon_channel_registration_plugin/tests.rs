use super::*;
use bevy_ecs::message::Message;
use bevy_replicon::prelude::{AuthMethod, RepliconSharedPlugin, ServerMessageAppExt};
use bevy_state::app::StatesPlugin;
use campfire_net_core::NetId;
use serde::{Deserialize, Serialize};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(StatesPlugin);
    app.add_plugins(RepliconSharedPlugin {
        auth_method: AuthMethod::None,
    });
    app.add_plugins(RepliconChannelRegistrationPlugin);
    app
}

fn type_ids(app: &App, channels: &[NetId]) -> Vec<ChannelTypeId> {
    let registry = app.world().resource::<ChannelRegistry>();
    channels
        .iter()
        .map(|&net_id| registry.type_id(net_id).unwrap())
        .collect()
}

/// Replicon's server channels 0 and 1 are its updates and mutations, and its client channel 0
/// is the mutation acks.
#[test]
fn maps_each_replicon_channel_once() {
    let mut app = app();
    app.finish();
    let map = app.world().resource::<RepliconChannelMap>();
    assert_eq!(
        type_ids(&app, &map.server_channels),
        [
            ChannelTypeId::of::<RepliconUpdatesChannel>(),
            ChannelTypeId::of::<RepliconMutationsChannel>()
        ]
    );
    assert_eq!(
        type_ids(&app, &map.client_channels),
        [ChannelTypeId::of::<RepliconMutationAcksChannel>()]
    );
}

#[derive(Message, Serialize, Deserialize)]
struct Chat;

/// A message registered with replicon adds a replicon channel that no stack channel
/// carries, which used to index out of bounds when it was sent.
#[test]
#[should_panic(expected = "register messages with the stack, not replicon")]
fn a_replicon_message_is_refused() {
    let mut app = app();
    app.add_server_message::<Chat>(Channel::Ordered);
    app.finish();
}
