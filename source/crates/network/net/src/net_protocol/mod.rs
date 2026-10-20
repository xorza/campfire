use std::fmt::Debug;

use bevy_app::{App, Plugin};
use bevy_ecs::component::{Component, Immutable, Mutable};
use campfire_capabilities::Release;
use campfire_declarations::CapabilitySet;
use campfire_net_channels::{Channel, ChannelMode, ChannelSetup};
use campfire_net_core::LinkDirection;
use campfire_net_messages::AppMessageExt;
use campfire_net_prediction::PredictionRegistrationExt;
use campfire_net_replication::AppComponentExt;
use campfire_net_transport::AppChannelExt;
use campfire_protocol::SignedReceipt;
use campfire_sim::{SimResource, StableId};
use campfire_states::{Declared, Replication, Sending, SendingWays, StateTypes};

use crate::net_protocol::dismissed::Dismissed;
use crate::net_protocol::input_ack::InputAck;
use crate::net_protocol::input_message::InputMessage;
use crate::net_protocol::join::Join;
use crate::net_protocol::joined::Joined;
use crate::net_protocol::leave_match::LeaveMatch;
use crate::net_protocol::match_start::MatchStart;
use crate::net_protocol::offer::Offer;
use crate::net_protocol::save_command::SaveCommand;
use crate::net_protocol::watch_start::WatchStart;
use crate::net_protocol::watch_teams::WatchTeams;

pub(crate) mod dismissed;
pub(crate) mod input_ack;
pub(crate) mod input_message;
pub(crate) mod join;
pub(crate) mod joined;
pub(crate) mod leave_match;
pub(crate) mod match_start;
pub(crate) mod offer;
pub(crate) mod save_command;
pub(crate) mod watch_start;
pub(crate) mod watch_teams;

/// Carries player inputs to the server. Reliable and ordered: the log refuses an input that does
/// not link to the one before it.
#[derive(Debug)]
pub struct InputChannel;

impl Channel for InputChannel {}

/// Carries the offer, the match start and a spectator's, the receipts, and the word that the
/// server ends the link, to the client.
#[derive(Debug)]
pub(crate) struct MatchChannel;

impl Channel for MatchChannel {}

/// Carries a player's join to the server, their word that they leave, and a spectator's choice
/// of teams.
#[derive(Debug)]
pub(crate) struct JoinChannel;

impl Channel for JoinChannel {}

/// What the server and the client must register alike, in the same order: the messages, their
/// channels, and the sim components that replicate, as the capabilities' lists name them, the
/// core's and those of the mode's declared capabilities alone, so a capability the mode lacks costs
/// no prediction history and no replication rule. The client predicts where its own units are,
/// where they walk to and by which route, which it plans on its own pathing grid, the forced moves
/// it learns of, which it continues as the server does, and their death and respawn, which it
/// learns from the server: its sim stops a dead unit and brings it back as the server's does, and a
/// rollback restores both. It learns each unit's type once and its level as it changes, and derives
/// its own units' stats, tags and step from them and their modifiers as the server does, so the
/// server sends a unit's step only with the unit. It starts their actions as the server does, and
/// learns their pools and the actions' effects from the server. It learns the teams' relations, the
/// match's end and each player's state on the match's and the players' entities, which replicate as
/// units do.
#[derive(Debug)]
pub struct NetProtocolPlugin {
    pub capabilities: CapabilitySet,
}

impl Plugin for NetProtocolPlugin {
    fn build(&self, app: &mut App) {
        let reliable = ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            ..ChannelSetup::default()
        };
        // A link receives inputs only once its player took a seat, so a peer that did not join
        // fills no input channel and holds no part of its link's receive budget with one.
        app.register_channel::<InputChannel>(reliable)
            .add_direction_after::<Joined>(LinkDirection::ClientToServer);
        app.register_channel::<MatchChannel>(reliable)
            .add_direction(LinkDirection::ServerToClient);
        app.register_channel::<JoinChannel>(reliable)
            .add_direction(LinkDirection::ClientToServer);
        app.register_message::<InputMessage>()
            .add_direction(LinkDirection::ClientToServer);
        app.register_message::<InputAck>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<MatchStart>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<Offer>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<Dismissed>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<SignedReceipt>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<Join>()
            .add_direction(LinkDirection::ClientToServer);
        app.register_message::<SaveCommand>()
            .add_direction(LinkDirection::ClientToServer);
        app.register_message::<LeaveMatch>()
            .add_direction(LinkDirection::ClientToServer);
        app.register_message::<WatchStart>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<WatchTeams>()
            .add_direction(LinkDirection::ClientToServer);

        // A unit's identity, which no state type holds: the entity list holds it.
        app.register_component::<StableId>().replicate_once();
        Release::state_types(self.capabilities, &mut Replicated(app));
    }
}

/// Reads the capabilities' lists for `NetProtocolPlugin`: registers each type of a kind a client
/// receives for replication, as its list says it is sent.
#[derive(Debug)]
struct Replicated<'a>(&'a mut App);

impl StateTypes for Replicated<'_> {
    fn component<C: Replication>(&mut self) {
        <C::Sending as Sending<C>>::visit(self);
    }

    fn sim_component<C: Replication>(&mut self) {
        self.component::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.0.register_component::<C>().replicate();
    }

    fn resource<R: SimResource + Default>(&mut self) {}
}

impl SendingWays for Replicated<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.0.register_component::<C>().replicate_once();
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.0.register_component::<C>().replicate();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.0.register_component::<C>().replicate();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.0.register_component::<C>().replicate().predict();
    }

    fn server<C: Replication>(&mut self, _: Declared) {}
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bevy_app::TaskPoolPlugin;
    use bevy_ecs::world::World;
    use bevy_state::app::StatesPlugin;
    use bevy_time::TimePlugin;
    use campfire_math::TickRate;
    use campfire_net_channels::ChannelTypeId;
    use campfire_net_connection::ClientOf;
    use campfire_net_replication::ComponentRegistry;
    use campfire_net_stack::ServerPlugins;
    use campfire_net_transport::{ChannelRegistry, Transport};

    use super::*;

    /// Whether `registry` registers each type the lists of a set send, by name.
    #[derive(Debug)]
    struct Registered<'a> {
        registry: &'a ComponentRegistry,
        types: BTreeMap<&'static str, bool>,
    }

    impl Registered<'_> {
        fn of(registry: &ComponentRegistry, set: CapabilitySet) -> BTreeMap<&'static str, bool> {
            let mut registered = Registered {
                registry,
                types: BTreeMap::new(),
            };
            Release::state_types(set, &mut registered);
            registered.types
        }

        fn add<C: Replication>(&mut self) {
            let held = self.registry.is_registered::<C>();
            self.types.insert(C::NAME, held);
        }
    }

    impl StateTypes for Registered<'_> {
        fn component<C: Replication>(&mut self) {
            <C::Sending as Sending<C>>::visit(self);
        }

        fn sim_component<C: Replication>(&mut self) {
            self.component::<C>();
        }

        fn derived<C: Component>(&mut self) {}

        fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
            self.add::<C>();
        }

        fn resource<R: SimResource + Default>(&mut self) {}
    }

    impl SendingWays for Registered<'_> {
        fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
            self.add::<C>();
        }

        fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
            self.add::<C>();
        }

        fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
            self.add::<C>();
        }

        fn predicted<
            C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug,
        >(
            &mut self,
            _: Declared,
        ) {
            self.add::<C>();
        }

        fn server<C: Replication>(&mut self, _: Declared) {}
    }

    /// A server's app with the protocol of the capabilities `set`.
    fn app_of(set: CapabilitySet) -> App {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        app.add_plugins(ServerPlugins {
            tick_rate: TickRate::try_from(20).unwrap(),
        });
        app.add_plugins(NetProtocolPlugin { capabilities: set });
        app
    }

    #[test]
    fn a_capability_the_mode_lacks_registers_no_component() {
        // A mode of no capability registers each type the core's lists send, and none of
        // another list's; one of every capability registers every type each list sends.
        let core = Release::capability_set(&[]).unwrap();
        let none = app_of(core);
        let registry = none.world().resource::<ComponentRegistry>();
        let own = Registered::of(registry, core);
        assert!(!own.is_empty() && own.values().all(|&held| held));
        let every = Registered::of(registry, Release::capabilities());
        assert!(every.len() > own.len());
        for (name, held) in every {
            assert_eq!(held, own.contains_key(name), "{name}");
        }
        let all = app_of(Release::capabilities());
        let registry = all.world().resource::<ComponentRegistry>();
        let every = Registered::of(registry, Release::capabilities());
        assert!(every.values().all(|&held| held));
    }

    /// A server's link receives on the join's channel from its start, and on the input channel
    /// only once it joined, and keeps it as `Joined` goes.
    #[test]
    fn a_link_receives_inputs_only_once_joined() {
        let mut app = app_of(Release::capability_set(&[]).unwrap());
        let world = app.world_mut();
        let link = world.spawn((ClientOf, Transport::default())).id();
        let channels = world.resource::<ChannelRegistry>();
        let [input, join] = [
            ChannelTypeId::of::<InputChannel>(),
            ChannelTypeId::of::<JoinChannel>(),
        ]
        .map(|channel| channels.net_id(channel).unwrap());
        let receives = |world: &mut World, channel| {
            world
                .get_mut::<Transport>(link)
                .unwrap()
                .channel_receiver_mut(channel)
                .is_some()
        };
        assert_eq!(
            [receives(world, input), receives(world, join)],
            [false, true]
        );
        world.entity_mut(link).insert(Joined);
        assert_eq!(
            [receives(world, input), receives(world, join)],
            [true, true]
        );
        world.entity_mut(link).remove::<Joined>();
        assert!(receives(world, input));
    }
}
