use bevy::ecs::system::{Res, SystemParam};
use campfire_common::Seat;
use campfire_net::{JoinState, WatchRequest};
use campfire_players::{PlayerResources, PlayerRows, ResourceAmount};
use campfire_units::{Commanded, PlayerUnits};
use campfire_values::{ResourceId, Team, TeamSet};

/// The client's own player as a system reads it: its seat and team, once the client plays, the
/// avatar it commands, its amounts of the player resources, and, for a spectator, the teams it
/// chose to watch.
#[derive(SystemParam, Debug)]
pub(crate) struct OwnPlayer<'w, 's> {
    state: Res<'w, JoinState>,
    players: PlayerUnits<'w, 's>,
    resources: PlayerRows<'w, 's, PlayerResources>,
    watch: Option<Res<'w, WatchRequest>>,
}

impl OwnPlayer<'_, '_> {
    /// The seat the client plays in; none before it plays.
    pub(crate) fn seat(&self) -> Option<Seat> {
        self.state.seat()
    }

    /// The team the client plays on; none before it plays.
    pub(crate) fn team(&self) -> Option<Team> {
        self.state.team()
    }

    /// The team whose units show as friends: the client's own once it plays, and a spectator's
    /// as `OwnPlayer::side_of` says; none before either.
    pub(crate) fn side(&self) -> Option<Team> {
        let watching = self.state.watching_clock().is_some();
        OwnPlayer::side_of(
            self.state.team(),
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
        let mut chosen = chosen.into_iter().flat_map(TeamSet::iter);
        match (chosen.next(), chosen.next()) {
            (Some(alone), None) => Some(alone),
            _ => Some(Team::new(0)),
        }
    }

    /// The player's amount of `resource`; 0 before it plays, or before its resources reach the
    /// client.
    pub(crate) fn amount(&self, resource: ResourceId) -> i64 {
        let held = self.seat().and_then(|seat| self.resources.of_seat(seat));
        held.map_or(0, |held| held.amount(resource))
    }

    /// Whether the player holds each of `costs`, as `amount` gives its amounts.
    pub(crate) fn affords(&self, costs: &[ResourceAmount]) -> bool {
        costs
            .iter()
            .all(|cost| self.amount(cost.resource) >= cost.amount)
    }

    /// The avatar the client commands; none before it plays, or before the avatar spawns.
    pub(crate) fn avatar(&self) -> Option<Commanded> {
        self.players.avatar(self.seat()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_sides_with_its_team_and_a_spectator_with_the_one_team_it_watches_or_the_first() {
        let (north, south) = (Team::new(0), Team::new(1));
        let both = TeamSet::from([north, south]);
        let alone = TeamSet::from([south]);
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
