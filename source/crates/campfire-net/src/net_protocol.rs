use bevy_app::{App, Plugin};
use campfire_capabilities::{
    ActionSlots, Area, Body, Dead, Destination, ForcedMove, Level, MatchEnd, ModifierClocks,
    Modifiers, MoveStep, Owner, Points, Pools, Progress, Projectile, Relations, Respawn, Route,
    SpawnPoint, Team, UnitType,
};
use campfire_protocol::SignedReceipt;
use campfire_sim::{Position, StableId};
use lightyear::prelude::{
    AppChannelExt, AppComponentExt, AppMessageExt, ChannelMode, ChannelSettings, NetworkDirection,
    PredictionBuilderExt, ReliableSettings,
};

use crate::input_message::InputMessage;
use crate::join::Join;
use crate::leave_match::LeaveMatch;
use crate::match_start::MatchStart;
use crate::offer::Offer;
use crate::superseded::Superseded;

/// Carries player inputs to the server. Reliable and ordered: the log refuses an input that does
/// not link to the one before it.
#[derive(Debug)]
pub struct InputChannel;

/// Carries the offer, the match start and end, the relations as they change, the receipts, and
/// the word that a newer login took the seat, to the client.
#[derive(Debug)]
pub(crate) struct MatchChannel;

/// Carries a player's join to the server, and their word that they leave.
#[derive(Debug)]
pub(crate) struct JoinChannel;

/// What the server and the client must register alike, in the same order: the messages, their
/// channels, and the sim components that replicate. The client predicts where its own units are,
/// where they walk to and by which route, which it plans on its own pathing grid, the forced moves
/// it learns of, which it continues as the server does, and their death and respawn, which it learns from the server: its sim stops a dead unit and brings it back as
/// the server's does, and a rollback restores both. It learns each unit's type once and its
/// level as it changes, and derives its own units' stats, tags and step from them and their
/// modifiers as the server does, so the server sends a unit's step only with the unit. It starts
/// their actions as the server does, and learns their pools and the actions' effects from the
/// server. It learns the teams' relations as a script changes them.
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
        app.register_message::<MatchEnd>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Relations>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Offer>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Superseded>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<SignedReceipt>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Join>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LeaveMatch>()
            .add_direction(NetworkDirection::ClientToServer);

        app.component::<StableId>().replicate_once();
        app.component::<UnitType>().replicate_once();
        app.component::<Level>().replicate().predict();
        app.component::<Owner>().replicate();
        app.component::<Team>().replicate_once();
        app.component::<SpawnPoint>().replicate_once();
        app.component::<Body>().replicate_once();
        app.component::<Pools>().replicate();
        app.component::<Projectile>().replicate_once();
        app.component::<Area>().replicate_once();
        app.component::<ActionSlots>().replicate().predict();
        app.component::<Points>().replicate().predict();
        app.component::<Dead>().replicate().predict();
        app.component::<Respawn>().replicate().predict();
        app.component::<MoveStep>().replicate_once();
        app.component::<Position>().replicate().predict();
        app.component::<Destination>().replicate().predict();
        app.component::<Route>().replicate().predict();
        app.component::<Progress>().replicate().predict();
        app.component::<ForcedMove>().replicate().predict();
        app.component::<Modifiers>().replicate().predict();
        app.component::<ModifierClocks>().replicate().predict();
    }
}
