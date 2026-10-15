use bevy_ecs::component::Component;
use campfire_common::PlayerSlot;
use campfire_values::Team;

/// Which player a client link carries the inputs of, their team, and whether the log refused one
/// of its messages, which ended the link. A link with one views the match as that player, and
/// only while it has one, as `Viewer` follows it. Immutable: a change inserts a new one.
#[derive(Component, Debug, Clone, Copy)]
#[component(immutable)]
pub struct PlayerLink {
    slot: PlayerSlot,
    team: Team,
    refused: bool,
}

impl PlayerLink {
    pub(crate) const fn new(slot: PlayerSlot, team: Team) -> PlayerLink {
        PlayerLink {
            slot,
            team,
            refused: false,
        }
    }

    pub const fn slot(self) -> PlayerSlot {
        self.slot
    }

    /// Whether the log refused one of its messages: a broken chain or signature, or a limit
    /// passed. A client that follows the rules sends none, so the first one ends the link.
    pub const fn refused(self) -> bool {
        self.refused
    }

    pub(crate) const fn team(self) -> Team {
        self.team
    }

    /// The link as the log refused one of its messages.
    pub(super) const fn into_refused(self) -> PlayerLink {
        PlayerLink {
            refused: true,
            ..self
        }
    }
}
