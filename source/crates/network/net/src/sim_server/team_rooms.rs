use bevy_ecs::resource::Resource;
use campfire_values::{NameId, Team, TeamSet};
use campfire_visibility::{OutView, SpectatorFog, Visibility};
use campfire_vision::SeenBy;
use lightyear::prelude::{RoomAllocator, RoomId, Rooms};

/// A Lightyear room for each team: a unit stands in the rooms of the teams that see it, and a
/// seated link in its team's, so Lightyear sends a link exactly the units its team sees, from
/// the moment it sits, and nothing to a link with no seat. A spectator stands in the rooms of the
/// teams it watches, or, in a session whose spectators see every unit, in the room every entity
/// stands in then (design 14, D15); a player out of play who sees as a spectator does, in its
/// team's and the spectators' (D16).
#[derive(Resource, Debug)]
pub(crate) struct TeamRooms {
    /// By team index, for every index a team may take.
    rooms: Vec<RoomId>,
    everything: RoomId,
    /// What the session's spectators see.
    fog: SpectatorFog,
    /// Whether a link may see as a spectator that sees every unit, so every entity stands in
    /// `everything`.
    spectators_see_everything: bool,
}

impl TeamRooms {
    /// Takes a room from `allocator` for each team index.
    pub(crate) fn new(allocator: &mut RoomAllocator) -> Self {
        let rooms = TeamSet::ALL.iter().map(|_| allocator.allocate()).collect();
        Self {
            rooms,
            everything: allocator.allocate(),
            fog: SpectatorFog::All,
            spectators_see_everything: false,
        }
    }

    /// Takes the spectators' fog of `visibility`, and puts every entity that replicates from now
    /// on in the room of the spectators that see every unit, in a session where a spectator, or a
    /// player out of play, may see so.
    pub(crate) fn set_spectators(&mut self, visibility: &Visibility) {
        self.fog = visibility.spectators();
        let watched = visibility.max_spectators() > 0 || visibility.out() == OutView::Spectators;
        self.spectators_see_everything = self.fog == SpectatorFog::All && watched;
    }

    /// The room of a spectator that sees every unit.
    pub(crate) fn everything(&self) -> Rooms {
        Rooms::single(self.everything)
    }

    /// The room of the link seated in `team`.
    pub(crate) fn of(&self, team: Team) -> Rooms {
        Rooms::single(self.rooms[team.index()])
    }

    /// The rooms of a player of `team` out of play who sees as a spectator does: its team's and
    /// the spectators', every unit's, or every team's view.
    pub(crate) fn out_of_play(&self, team: Team) -> Rooms {
        match self.fog {
            SpectatorFog::All => {
                Rooms::from([self.rooms[team.index()], self.everything].into_iter())
            }
            SpectatorFog::Teams => self.watching(TeamSet::ALL),
        }
    }

    /// The rooms of a spectator that watches `teams`.
    pub(crate) fn watching(&self, teams: TeamSet) -> Rooms {
        Rooms::from(teams.iter().map(|team| self.rooms[team.index()]))
    }

    /// The rooms of a unit that `seen` says which teams see: every team's for a unit of which
    /// no Vision stage spoke, or that has no `SeenBy`, as in a match with no vision.
    pub(crate) fn seeing(&self, seen: Option<&SeenBy>) -> Rooms {
        let teams = seen.and_then(|seen| seen.get()).unwrap_or(TeamSet::ALL);
        let everything = self.spectators_see_everything.then_some(self.everything);
        let rooms = teams.iter().map(|team| self.rooms[team.index()]);
        Rooms::from(rooms.chain(everything))
    }
}
