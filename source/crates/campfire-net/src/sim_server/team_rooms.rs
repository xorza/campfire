use bevy_ecs::resource::Resource;
use campfire_capabilities::{SeenBy, Team, TeamSet};
use lightyear::prelude::{RoomAllocator, RoomId, Rooms};

/// A Lightyear room for each team: a unit stands in the rooms of the teams that see it, and a
/// seated link in its team's, so Lightyear sends a link exactly the units its team sees, from
/// the moment it sits, and nothing to a link with no seat.
#[derive(Resource, Debug)]
pub(crate) struct TeamRooms {
    /// By team index, for every index a team may take.
    rooms: Vec<RoomId>,
}

impl TeamRooms {
    /// Takes a room from `allocator` for each team index.
    pub(crate) fn new(allocator: &mut RoomAllocator) -> TeamRooms {
        let rooms = TeamSet::ALL.teams().map(|_| allocator.allocate()).collect();
        TeamRooms { rooms }
    }

    /// The room of the link seated in `team`.
    pub(crate) fn of(&self, team: Team) -> Rooms {
        Rooms::single(self.rooms[team.index()])
    }

    /// The rooms of a unit that `seen` says which teams see: every team's for a unit with none,
    /// as in a match with no vision.
    pub(crate) fn seeing(&self, seen: Option<&SeenBy>) -> Rooms {
        let teams = seen.map_or(TeamSet::ALL, |seen| seen.get());
        Rooms::from(teams.teams().map(|team| self.rooms[team.index()]))
    }
}
