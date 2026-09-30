use bevy_app::{App, Plugin};
use campfire_capabilities::{
    AbilitySlots, Dead, Destination, Health, MoveStep, Owner, ResourcePool, Respawn, SpawnPoint,
    Team,
};
use campfire_sim::{Position, StableId};
use lightyear::prelude::{
    AppChannelExt, AppComponentExt, AppMessageExt, ChannelMode, ChannelSettings, NetworkDirection,
    PredictionBuilderExt, ReliableSettings,
};

use crate::input_message::InputMessage;
use crate::join::Join;
use crate::match_start::MatchStart;
use crate::offer::Offer;

/// Carries player inputs to the server. Reliable and ordered: the log refuses an input that does
/// not link to the one before it.
#[derive(Debug)]
pub struct InputChannel;

/// Carries the offer and the match start to the client.
#[derive(Debug)]
pub struct MatchChannel;

/// Carries a player's join to the server.
#[derive(Debug)]
pub struct JoinChannel;

/// What the server and the client must register alike, in the same order: the messages, their
/// channels, and the sim components that replicate. The client predicts where its own units are
/// and where they walk to, and their death and respawn, which it learns from the server: its sim
/// stops a dead unit and brings it back as the server's does, and a rollback restores both. It
/// learns their health, resource and ability slots from the server: it predicts no casts.
#[derive(Debug)]
pub struct NetProtocol;

impl Plugin for NetProtocol {
    fn build(&self, app: &mut App) {
        let reliable = || ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..ChannelSettings::default()
        };
        app.add_channel::<InputChannel>(reliable())
            .add_direction(NetworkDirection::ClientToServer);
        app.add_channel::<MatchChannel>(reliable())
            .add_direction(NetworkDirection::ServerToClient);
        app.add_channel::<JoinChannel>(reliable())
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<InputMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<MatchStart>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Offer>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Join>()
            .add_direction(NetworkDirection::ClientToServer);

        app.component::<StableId>().replicate_once();
        app.component::<Owner>().replicate();
        app.component::<Team>().replicate_once();
        app.component::<SpawnPoint>().replicate_once();
        app.component::<Health>().replicate();
        app.component::<ResourcePool>().replicate();
        app.component::<AbilitySlots>().replicate();
        app.component::<Dead>().replicate().predict();
        app.component::<Respawn>().replicate().predict();
        app.component::<MoveStep>().replicate();
        app.component::<Position>().replicate().predict();
        app.component::<Destination>().replicate().predict();
    }
}
