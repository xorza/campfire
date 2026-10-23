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

use campfire_host::Dismissed;
use campfire_host::InputAck;
use campfire_host::InputMessage;
use campfire_host::Join;
use campfire_host::Joined;
use campfire_host::LeaveMatch;
use campfire_host::MatchLoaded;
use campfire_host::MatchStart;
use campfire_host::SaveCommand;
use campfire_host::SessionOffer;
use campfire_host::WatchStart;
use campfire_host::WatchTeams;

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
        // A link receives inputs only once its player took a player slot, so a peer that did not
        // join fills no input channel and holds no part of its link's receive budget with one.
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
        app.register_message::<SessionOffer>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<Dismissed>()
            .add_direction(LinkDirection::ServerToClient);
        app.register_message::<MatchLoaded>()
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
mod tests;
