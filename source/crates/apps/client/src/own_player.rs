use bevy::ecs::query::Allow;
use bevy::ecs::system::{Query, Res, SystemParam};
use campfire_common::PlayerSlot;
use campfire_host::JoinState;
use campfire_players::{PlayerResources, PlayerRows, ResourceAmount};
use campfire_sim::Unpredicted;
use campfire_units::{Commanded, PlayerUnits};
use campfire_values::{Relation, Relations, ResourceId, Team, TeamSet};

/// The client's own player as a system reads it: its player slot and team, once the client plays,
/// the avatar it commands, its amounts of the player resources, how its side regards each team by
/// the relations on the match's entity, and, for a spectator, the teams it chose to watch.
#[derive(SystemParam, Debug)]
pub(crate) struct OwnPlayer<'w, 's> {
    state: Res<'w, JoinState>,
    players: PlayerUnits<'w, 's>,
    resources: PlayerRows<'w, 's, PlayerResources>,
    relations: Query<'w, 's, &'static Relations, Allow<Unpredicted>>,
}

impl OwnPlayer<'_, '_> {
    /// The player slot the client plays in; none before it plays.
    pub(crate) fn player_slot(&self) -> Option<PlayerSlot> {
        self.state.player_slot()
    }

    /// The team whose units show as friends: the client's own once it plays, and a spectator's
    /// as `OwnPlayer::side_of` says; none before either.
    pub(crate) fn side(&self) -> Option<Team> {
        OwnPlayer::side_of(self.state.team(), self.state.watched_teams())
    }

    /// How the client's side regards `team`, by the match's relations; none before it plays or
    /// watches, or before the relations reach it.
    pub(crate) fn relation_to(&self, team: Team) -> Option<Relation> {
        let relations = self.relations.single().ok()?;
        Some(relations.between(self.side()?, team))
    }

    /// The side of a client that plays on `team`, or watches the teams of `watched`: its own
    /// team; a spectator's the one team it watches alone, else the first team, as Dota 2's
    /// spectators see the Radiant's units in green.
    fn side_of(team: Option<Team>, watched: Option<TeamSet>) -> Option<Team> {
        if team.is_some() {
            return team;
        }
        let mut watched = watched?.iter();
        match (watched.next(), watched.next()) {
            (Some(alone), None) => Some(alone),
            _ => Some(Team::new(0)),
        }
    }

    /// The player's amount of `resource`; 0 before it plays, or before its resources reach the
    /// client.
    pub(crate) fn amount(&self, resource: ResourceId) -> i64 {
        let held = self
            .player_slot()
            .and_then(|player_slot| self.resources.of_player_slot(player_slot));
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
        self.players.avatar(self.player_slot()?)
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
        for (team, watched, side) in [
            // A player; a client that neither plays nor watches.
            (Some(south), None, Some(south)),
            (None, None, None),
            // A spectator: of every team, as it joins; of two; of the south alone.
            (None, Some(TeamSet::ALL), Some(north)),
            (None, Some(both), Some(north)),
            (None, Some(alone), Some(south)),
        ] {
            assert_eq!(
                OwnPlayer::side_of(team, watched),
                side,
                "{team:?} {watched:?}"
            );
        }
    }
}
