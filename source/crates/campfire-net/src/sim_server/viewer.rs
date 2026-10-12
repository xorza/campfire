use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use campfire_capabilities::{Recipients, Team, TeamSet};
use campfire_common::PlayerSlot;
use lightyear::prelude::Rooms;

use crate::sim_server::team_rooms::TeamRooms;

/// How a client link views the match, which every kind's filter reads (design 14, D1, D15): as
/// the player of a slot of a team, or as a spectator. It stands in its rooms by it, and in none
/// without one. Immutable, as replicon's filters require: a change inserts a new one.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable, on_insert = Viewer::enter_rooms, on_remove = Viewer::leave_rooms)]
pub(crate) enum Viewer {
    Player { slot: PlayerSlot, team: Team },
    Spectator(Watching),
}

/// What a spectator watches: every unit, through any fog, or the views of the teams it chose
/// together (design 14, D15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Watching {
    Everything,
    Teams(TeamSet),
}

impl Viewer {
    /// The recipient it is of an entity `owner` owns, of `team`: none for the owner, who receives
    /// every sent kind; an ally of its own team, or of an entity of no team, as the match's;
    /// an enemy of another; a spectator.
    pub(crate) fn recipient(self, owner: Option<PlayerSlot>, team: Option<Team>) -> Recipients {
        match self {
            Viewer::Player { slot, .. } if Some(slot) == owner => Recipients::NONE,
            Viewer::Player { team: own, .. } => match team {
                Some(team) if team != own => Recipients::ENEMIES,
                _ => Recipients::ALLIES,
            },
            Viewer::Spectator(_) => Recipients::SPECTATORS,
        }
    }

    fn enter_rooms(mut world: DeferredWorld<'_>, context: HookContext) {
        let viewer = *world
            .get::<Viewer>(context.entity)
            .expect("on_insert runs on an entity with a Viewer");
        let rooms = world.resource::<TeamRooms>();
        let rooms = match viewer {
            Viewer::Player { team, .. } => rooms.of(team),
            Viewer::Spectator(Watching::Everything) => rooms.everything(),
            Viewer::Spectator(Watching::Teams(teams)) => rooms.watching(teams),
        };
        world.commands().entity(context.entity).insert(rooms);
    }

    fn leave_rooms(mut world: DeferredWorld<'_>, context: HookContext) {
        world
            .commands()
            .entity(context.entity)
            .try_remove::<Rooms>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_viewer_is_the_owner_an_ally_an_enemy_or_a_spectator() {
        let (slot, team) = (PlayerSlot::new, Team::new);
        let player = Viewer::Player {
            slot: slot(1),
            team: team(0),
        };
        // Of a unit of slot 1's: none, as the owner; of slot 2's of its team, an ally; of the
        // other team's, an enemy; of the match's, of no team or owner, an ally.
        assert_eq!(
            player.recipient(Some(slot(1)), Some(team(0))),
            Recipients::NONE
        );
        assert_eq!(
            player.recipient(Some(slot(2)), Some(team(0))),
            Recipients::ALLIES
        );
        assert_eq!(player.recipient(None, Some(team(1))), Recipients::ENEMIES);
        assert_eq!(player.recipient(None, None), Recipients::ALLIES);
        let spectator = Viewer::Spectator(Watching::Teams(TeamSet::ALL));
        for (owner, of) in [
            (Some(slot(1)), Some(team(0))),
            (None, Some(team(1))),
            (None, None),
        ] {
            assert_eq!(spectator.recipient(owner, of), Recipients::SPECTATORS);
        }
    }
}
