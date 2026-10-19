use bevy_ecs::component::Component;
use campfire_common::Seat;
use campfire_values::Team;

/// Which player a client link carries the inputs of, their team, and whether the log refused one
/// of its messages, which ended the link. A link with one views the match as that player, and
/// only while it has one, as `Viewer` follows it. Immutable: a change inserts a new one.
#[derive(Component, Debug, Clone, Copy)]
#[component(immutable)]
pub(crate) struct PlayerLink {
    seat: Seat,
    team: Team,
    refused: bool,
}

impl PlayerLink {
    pub(crate) const fn new(seat: Seat, team: Team) -> Self {
        Self {
            seat,
            team,
            refused: false,
        }
    }

    pub(crate) const fn seat(self) -> Seat {
        self.seat
    }

    /// Whether the log refused one of its messages: a broken chain or signature, or a limit
    /// passed. A client that follows the rules sends none, so the first one ends the link.
    pub(crate) const fn is_refused(self) -> bool {
        self.refused
    }

    pub(crate) const fn team(self) -> Team {
        self.team
    }

    /// The link as the log refused one of its messages.
    pub(super) const fn into_refused(self) -> Self {
        Self {
            refused: true,
            ..self
        }
    }
}
