use bevy_ecs::resource::Resource;
use campfire_capabilities::{SeenBy, SpectatorFog, Team, TeamSet, Visibility};
use lightyear::prelude::{RoomAllocator, RoomId, Rooms};

/// A Lightyear room for each team: a unit stands in the rooms of the teams that see it, and a
/// seated link in its team's, so Lightyear sends a link exactly the units its team sees, from
/// the moment it sits, and nothing to a link with no seat. A spectator stands in the rooms of the
/// teams it watches, or, in a session whose spectators see every unit, in the room every entity
/// stands in then (design 14, D15).
#[derive(Resource, Debug)]
pub(crate) struct TeamRooms {
    /// By team index, for every index a team may take.
    rooms: Vec<RoomId>,
    everything: RoomId,
    /// Whether the session's spectators see every unit, so every entity stands in `everything`.
    spectators_see_everything: bool,
}

impl TeamRooms {
    /// Takes a room from `allocator` for each team index.
    pub(crate) fn new(allocator: &mut RoomAllocator) -> TeamRooms {
        let rooms = TeamSet::ALL.teams().map(|_| allocator.allocate()).collect();
        TeamRooms {
            rooms,
            everything: allocator.allocate(),
            spectators_see_everything: false,
        }
    }

    /// Puts every entity that replicates from now on in the room of the spectators that see
    /// every unit, in a session of `visibility` that admits such spectators.
    pub(crate) fn set_spectators(&mut self, visibility: &Visibility) {
        self.spectators_see_everything =
            visibility.spectators() == SpectatorFog::All && visibility.max_spectators() > 0;
    }

    /// The room of a spectator that sees every unit.
    pub(crate) fn everything(&self) -> Rooms {
        Rooms::single(self.everything)
    }

    /// The room of the link seated in `team`.
    pub(crate) fn of(&self, team: Team) -> Rooms {
        Rooms::single(self.rooms[team.index()])
    }

    /// The rooms of a spectator that watches `teams`.
    pub(crate) fn watching(&self, teams: TeamSet) -> Rooms {
        Rooms::from(teams.teams().map(|team| self.rooms[team.index()]))
    }

    /// The rooms of a unit that `seen` says which teams see: every team's for a unit with none,
    /// as in a match with no vision.
    pub(crate) fn seeing(&self, seen: Option<&SeenBy>) -> Rooms {
        let teams = seen.map_or(TeamSet::ALL, |seen| seen.get());
        let everything = self.spectators_see_everything.then_some(self.everything);
        let rooms = teams.teams().map(|team| self.rooms[team.index()]);
        Rooms::from(rooms.chain(everything))
    }
}
