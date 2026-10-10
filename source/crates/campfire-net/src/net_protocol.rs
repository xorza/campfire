use std::time::Duration;

use bevy_app::{App, Plugin};
use campfire_capabilities::{
    ActionSlots, Area, Body, CapabilitySet, Dead, Destination, ForcedMove, Level, MatchEnd,
    ModifierClocks, Modifiers, MoveStep, Owner, Points, Pools, Progress, Projectile, Relations,
    Respawn, Route, SpawnPoint, Team, UnitType,
};
use campfire_protocol::SignedReceipt;
use campfire_sim::{Capability, Position, StableId};
use lightyear::prelude::{
    AppChannelExt, AppComponentExt, AppMessageExt, ChannelMode, ChannelSettings, NetworkDirection,
    PredictionBuilderExt, ReliableSettings,
};

use crate::input_message::InputMessage;
use crate::join::Join;
use crate::leave_match::LeaveMatch;
use crate::match_start::MatchStart;
use crate::offer::Offer;
use crate::save_command::SaveCommand;
use crate::superseded::Superseded;
use crate::wire_codec::WireCodec;

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
/// channels, and the sim components that replicate, the core's and those of the mode's declared
/// capabilities alone, so a capability the mode lacks costs no prediction history and no
/// replication rule. The client predicts where its own units are, where they walk to and by which
/// route, which it plans on its own pathing grid, the forced moves it learns of, which it continues
/// as the server does, and their death and respawn, which it learns from the server: its sim stops
/// a dead unit and brings it back as the server's does, and a rollback restores both. It learns
/// each unit's type once and its level as it changes, and derives its own units' stats, tags and
/// step from them and their modifiers as the server does, so the server sends a unit's step only
/// with the unit. It starts their actions as the server does, and learns their pools and the
/// actions' effects from the server. It learns the teams' relations as it sits, and again as a
/// script changes them.
#[derive(Debug)]
pub struct NetProtocol {
    pub capabilities: CapabilitySet,
}

impl NetProtocol {
    /// How often a headless app's loop runs, a server's or a bot's: often enough that no fixed
    /// tick waits long for its frame.
    pub const FRAME: Duration = Duration::from_millis(2);

    /// Registers the sim components `capability` replicates.
    fn register_components(app: &mut App, capability: Capability) {
        match capability {
            Capability::Stats => {
                app.component::<Level>()
                    .replicate_with(WireCodec::component())
                    .predict();
                app.component::<Pools>()
                    .replicate_with(WireCodec::component());
                app.component::<Modifiers>()
                    .replicate_with(WireCodec::component())
                    .predict();
                app.component::<ModifierClocks>()
                    .replicate_with(WireCodec::component())
                    .predict();
            }
            Capability::Combat => {
                app.component::<Dead>()
                    .replicate_with(WireCodec::component())
                    .predict();
                app.component::<Respawn>()
                    .replicate_with(WireCodec::component())
                    .predict();
            }
            Capability::Projectiles => {
                app.component::<Projectile>()
                    .replicate_once_with(WireCodec::component());
            }
            Capability::Areas => {
                app.component::<Area>()
                    .replicate_once_with(WireCodec::component());
            }
            Capability::Navigation => {
                app.component::<MoveStep>()
                    .replicate_once_with(WireCodec::component());
                app.component::<Destination>()
                    .replicate_with(WireCodec::component())
                    .predict();
                app.component::<Route>()
                    .replicate_with(WireCodec::component())
                    .predict();
                app.component::<Progress>()
                    .replicate_with(WireCodec::component())
                    .predict();
                app.component::<ForcedMove>()
                    .replicate_with(WireCodec::component())
                    .predict();
            }
            Capability::Progression => {
                app.component::<Points>()
                    .replicate_with(WireCodec::component())
                    .predict();
            }
            Capability::Abilities
            | Capability::Orders
            | Capability::Character
            | Capability::Hitboxes
            | Capability::Vision
            | Capability::Physics
            | Capability::World
            | Capability::Mode
            | Capability::Production
            | Capability::Items
            | Capability::Quests
            | Capability::Interaction => {}
        }
    }
}

impl Plugin for NetProtocol {
    fn build(&self, app: &mut App) {
        // Lightyear resends a message 1.5 round trips after it went out unacknowledged, and never
        // when that is zero, as a round trip measured in process reads; the floor keeps a resend
        // on such a link, and is below 1.5 round trips of any link across a network.
        let reliable = || ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings {
                rtt_resend_min_delay: Duration::from_millis(20),
                ..ReliableSettings::default()
            }),
            ..ChannelSettings::default()
        };
        app.add_channel::<InputChannel>(reliable())
            .add_direction(NetworkDirection::ClientToServer);
        app.add_channel::<MatchChannel>(reliable())
            .add_direction(NetworkDirection::ServerToClient);
        app.add_channel::<JoinChannel>(reliable())
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message_custom_serde::<InputMessage>(WireCodec::message())
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message_custom_serde::<MatchStart>(WireCodec::message())
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message_custom_serde::<MatchEnd>(WireCodec::message())
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message_custom_serde::<Relations>(WireCodec::message())
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message_custom_serde::<Offer>(WireCodec::message())
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message_custom_serde::<Superseded>(WireCodec::message())
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message_custom_serde::<SignedReceipt>(WireCodec::message())
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message_custom_serde::<Join>(WireCodec::message())
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message_custom_serde::<SaveCommand>(WireCodec::message())
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message_custom_serde::<LeaveMatch>(WireCodec::message())
            .add_direction(NetworkDirection::ClientToServer);

        app.component::<StableId>()
            .replicate_once_with(WireCodec::component());
        app.component::<UnitType>()
            .replicate_once_with(WireCodec::component());
        app.component::<Owner>()
            .replicate_with(WireCodec::component());
        app.component::<Team>()
            .replicate_once_with(WireCodec::component());
        app.component::<SpawnPoint>()
            .replicate_once_with(WireCodec::component());
        app.component::<Body>()
            .replicate_once_with(WireCodec::component());
        app.component::<Position>()
            .replicate_with(WireCodec::component())
            .predict();
        app.component::<ActionSlots>()
            .replicate_with(WireCodec::component())
            .predict();
        for capability in self.capabilities.iter() {
            NetProtocol::register_components(app, capability);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy_app::TaskPoolPlugin;
    use bevy_state::app::StatesPlugin;
    use bevy_time::TimePlugin;
    use lightyear::prelude::ComponentRegistry;
    use lightyear::prelude::server::ServerPlugins;

    use super::*;

    #[test]
    fn a_capability_the_mode_lacks_registers_no_component() {
        // Of each kind: the core's, stats', combat's, navigation's, progression's, projectiles'.
        let registered = |declared: &[Capability]| {
            let mut app = App::new();
            app.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
            app.add_plugins(ServerPlugins {
                tick_duration: Duration::from_millis(50),
            });
            app.add_plugins(NetProtocol {
                capabilities: CapabilitySet::new(declared).unwrap(),
            });
            let registry = app.world().resource::<ComponentRegistry>();
            [
                registry.is_registered::<Position>(),
                registry.is_registered::<Level>(),
                registry.is_registered::<Dead>(),
                registry.is_registered::<Destination>(),
                registry.is_registered::<Points>(),
                registry.is_registered::<Projectile>(),
            ]
        };
        assert_eq!(registered(&[]), [true, false, false, false, false, false]);
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Progression,
            Capability::Projectiles,
        ];
        assert_eq!(registered(&declared), [true; 6]);
    }
}
