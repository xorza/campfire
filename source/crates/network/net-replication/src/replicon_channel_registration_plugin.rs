use crate::replicon_channel_map::RepliconChannelMap;
use crate::replicon_mutation_acks_channel::RepliconMutationAcksChannel;
use crate::replicon_mutations_channel::RepliconMutationsChannel;
use crate::replicon_updates_channel::RepliconUpdatesChannel;
use bevy_app::prelude::*;
use bevy_replicon::shared::backend::channels::{Channel, RepliconChannels};
use campfire_net_channels::ChannelMode;
use campfire_net_channels::ChannelSetup;
use campfire_net_channels::ChannelTypeId;
use campfire_net_core::LinkDirection;
use campfire_net_transport::{AppChannelExt, ChannelRegistry};

/// Registers Replicon's channels as the transport's, with their map (13-net-stack/transport.md,
/// Replicon's channels map from `RepliconChannels`).
#[derive(Debug)]
pub(crate) struct RepliconChannelRegistrationPlugin;

impl Plugin for RepliconChannelRegistrationPlugin {
    fn build(&self, app: &mut App) {
        // ServerChannel::Updates - Ordered Reliable
        // High priority: entity actions (spawn/despawn/insert/remove) should be sent ASAP
        app.register_channel::<RepliconUpdatesChannel>(ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            priority: 10.0,
            ..Default::default()
        })
        .add_direction(LinkDirection::ServerToClient);

        // ServerChannel::Mutations - Unreliable
        // Low priority: component mutations can be dropped if bandwidth is limited because a
        // later mutation contains fresher state.
        app.register_channel::<RepliconMutationsChannel>(ChannelSetup {
            mode: ChannelMode::UnorderedUnreliable,
            priority: 1.0,
            // A later mutation contains fresher component state, so do not build a stale backlog.
            retry_unsent_messages: false,
            ..Default::default()
        })
        .add_direction(LinkDirection::ServerToClient);

        // ClientChannel::MutationAcks - Ordered Reliable
        // High priority: acks should be sent ASAP to avoid unnecessary resends
        app.register_channel::<RepliconMutationAcksChannel>(ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            priority: 10.0,
            ..Default::default()
        })
        .add_direction(LinkDirection::ClientToServer);
    }

    fn finish(&self, app: &mut App) {
        // The stack carries replicon's replication channels only: messages and events travel
        // through the stack's own registration, which checks their direction.
        let replicon = app.world().resource::<RepliconChannels>();
        assert_eq!(
            replicon.server_channels(),
            [Channel::Ordered, Channel::Unreliable],
            "replicon's server channels are its two replication channels; register messages \
             with the stack, not replicon"
        );
        assert_eq!(
            replicon.client_channels(),
            [Channel::Ordered],
            "replicon's client channel is its mutation acks; register messages with the stack, \
             not replicon"
        );

        let registry = app.world().resource::<ChannelRegistry>();
        let entry = |type_id: ChannelTypeId| registry.net_id(type_id).unwrap();
        let server_channels = std::vec![
            entry(ChannelTypeId::of::<RepliconUpdatesChannel>()),
            entry(ChannelTypeId::of::<RepliconMutationsChannel>()),
        ];
        let client_channels = std::vec![entry(ChannelTypeId::of::<RepliconMutationAcksChannel>())];

        app.insert_resource(RepliconChannelMap {
            server_channels,
            client_channels,
        });
    }
}

#[cfg(test)]
mod tests {
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
}
