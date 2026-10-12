use bevy::ecs::system::{Res, SystemParam};
use campfire_capabilities::{Commanded, PlayerUnits, Team, TeamSet};
use campfire_common::PlayerSlot;
use campfire_net::{JoinState, WatchRequest};

/// The client's own player as a system reads it: its seat and team, once the client plays, the
/// avatar it commands, and, for a spectator, the teams it chose to watch.
#[derive(SystemParam, Debug)]
pub(crate) struct OwnPlayer<'w, 's> {
    state: Option<Res<'w, JoinState>>,
    players: PlayerUnits<'w, 's>,
    watch: Option<Res<'w, WatchRequest>>,
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

    /// The team whose units show as friends: the client's own once it plays, and a spectator's
    /// as `OwnPlayer::side_of` says; none before either.
    pub(crate) fn side(&self) -> Option<Team> {
        let state = self.state.as_deref()?;
        let watching = state.watching().is_some();
        OwnPlayer::side_of(
            state.team(),
            watching,
            self.watch.as_deref().map(|watch| watch.0),
        )
    }

    /// The side of a client that plays on `team`, or watches when `watching`, the teams of
    /// `chosen` if it chose: its own team; a spectator's the one team it watches alone, else the
    /// first team, as Dota 2's spectators see the Radiant's units in green.
    fn side_of(team: Option<Team>, watching: bool, chosen: Option<TeamSet>) -> Option<Team> {
        if team.is_some() || !watching {
            return team;
        }
        let mut chosen = chosen.into_iter().flat_map(TeamSet::teams);
        match (chosen.next(), chosen.next()) {
            (Some(alone), None) => Some(alone),
            _ => Some(Team::new(0)),
        }
    }

    /// The avatar the client commands; none before it plays, or before the avatar spawns.
    pub(crate) fn avatar(&self) -> Option<Commanded> {
        self.players.avatar(self.slot()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_sides_with_its_team_and_a_spectator_with_the_one_team_it_watches_or_the_first() {
        let (north, south) = (Team::new(0), Team::new(1));
        let both = TeamSet::from_iter([north, south]);
        let alone = TeamSet::from_iter([south]);
        for (team, watching, chosen, side) in [
            // A player, whatever it watches; a client that neither plays nor watches.
            (Some(south), false, None, Some(south)),
            (None, false, Some(alone), None),
            // A spectator: of every team, as it joins; of two; of the south alone.
            (None, true, None, Some(north)),
            (None, true, Some(both), Some(north)),
            (None, true, Some(alone), Some(south)),
        ] {
            assert_eq!(
                OwnPlayer::side_of(team, watching, chosen),
                side,
                "{team:?} {watching} {chosen:?}"
            );
        }
    }
}
