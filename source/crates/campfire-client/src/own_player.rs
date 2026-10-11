use bevy::ecs::system::{Res, SystemParam};
use campfire_capabilities::{Commanded, PlayerUnits, Team};
use campfire_common::PlayerSlot;
use campfire_net::JoinState;

/// The client's own player as a system reads it: its seat and team, once the client plays, and
/// the avatar it commands.
#[derive(SystemParam, Debug)]
pub(crate) struct OwnPlayer<'w, 's> {
    state: Option<Res<'w, JoinState>>,
    players: PlayerUnits<'w, 's>,
}

impl OwnPlayer<'_, '_> {
    /// The seat the client plays in; none before it plays.
    pub(crate) fn slot(&self) -> Option<PlayerSlot> {
        self.state.as_deref()?.slot()
    }

    /// The team the client plays on; none before it plays.
    pub(crate) fn team(&self) -> Option<Team> {
        self.state.as_deref()?.team()
    }

    /// The avatar the client commands; none before it plays, or before the avatar spawns.
    pub(crate) fn avatar(&self) -> Option<Commanded> {
        self.players.avatar(self.slot()?)
    }
}
