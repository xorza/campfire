use bevy_app::{App, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_declarations::CapabilitySet;
use campfire_math::TickRate;
use campfire_net_connection::{Client, Connect};
use campfire_net_endpoint::Endpoint;
use campfire_net_link::{Link, Linked, Unlinked};
use campfire_net_messages::MessageReceiver;
use campfire_net_packets::DatagramIo;
use campfire_net_payload::UnlinkReason;
use campfire_net_stack::ClientPlugins;

use crate::events::listening::Listening;
use crate::net_protocol::NetProtocolPlugin;
use crate::net_protocol::offer::SessionOffer;

/// A client of a session's protocol and nothing past it, on its own app: a test of a server's
/// process links it to see the server's links and closes, as no match runs on it.
#[derive(Debug)]
pub struct ProtocolClient {
    app: App,
    entity: Entity,
}

/// What a protocol client's frames left: whether it is linked, whether a session offered itself
/// to it, and how its link ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolClientState {
    pub linked: bool,
    pub offered: bool,
    pub end: Option<UnlinkReason>,
}

/// Whether a session's offer reached the client, which reads each offer as a joining client
/// does.
#[derive(Resource, Debug, Default)]
struct Offered(bool);

impl Offered {
    fn read(
        mut receivers: Query<'_, '_, &mut MessageReceiver<SessionOffer>>,
        mut offered: ResMut<'_, Self>,
    ) {
        for mut receiver in &mut receivers {
            offered.0 |= receiver.receive().count() > 0;
        }
    }
}

impl ProtocolClient {
    /// A client of the protocol of a mode of `capabilities`, at the rate of the server
    /// `listening` names, which connects through `endpoint`.
    pub fn connect<Io: DatagramIo>(
        endpoint: Endpoint<Io>,
        listening: &Listening,
        capabilities: CapabilitySet,
    ) -> Self {
        let tick_rate = TickRate::try_from(listening.tick_hz.get()).unwrap();
        let mut app = App::new();
        app.add_plugins((OneThreadPlugins, TimePlugin, StatesPlugin));
        app.add_plugins(ClientPlugins { tick_rate });
        app.add_plugins(NetProtocolPlugin { capabilities });
        app.init_resource::<Offered>()
            .add_systems(Update, Offered::read);
        app.finish();
        app.cleanup();
        let entity = app
            .world_mut()
            .spawn((Client, Link::default(), endpoint))
            .id();
        app.world_mut().trigger(Connect { entity });
        Self { app, entity }
    }

    /// Runs a frame.
    pub fn update(&mut self) -> ProtocolClientState {
        self.app.update();
        let world = self.app.world();
        ProtocolClientState {
            linked: world.get::<Linked>(self.entity).is_some(),
            offered: world.resource::<Offered>().0,
            end: world
                .get::<Unlinked>(self.entity)
                .map(|unlinked| unlinked.reason)
                .filter(|&reason| reason != UnlinkReason::Initial),
        }
    }
}
