use bevy_ecs::component::Component;
use campfire_common::PlayerSlot;
use campfire_values::Team;

/// Which player a client link carries the inputs of, their team, and whether the log refused one
/// of its messages, which ended the link. A link with one views the match as that player, and
/// only while it has one, as `Viewer` follows it. Immutable: a change inserts a new one.
#[derive(Component, Debug, Clone, Copy)]
#[component(immutable)]
pub struct PlayerLink {
    player_slot: PlayerSlot,
    team: Team,
    refused: bool,
}

impl PlayerLink {
    pub const fn new(player_slot: PlayerSlot, team: Team) -> Self {
        Self {
            player_slot,
            team,
            refused: false,
        }
    }

    pub const fn player_slot(self) -> PlayerSlot {
        self.player_slot
    }

    /// Whether the log refused one of its messages: a broken chain or signature, or a limit
    /// passed. A client that follows the rules sends none, so the first one ends the link.
    pub const fn is_refused(self) -> bool {
        self.refused
    }

    pub const fn team(self) -> Team {
        self.team
    }

    /// The link as the log refused one of its messages.
    #[must_use]
    pub const fn into_refused(self) -> Self {
        Self {
            refused: true,
            ..self
        }
    }
}
