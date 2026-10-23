use std::fmt;

use bevy_app::App;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::system::{Commands, Query, Res, SystemParam};
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::PlayerSlot;
use campfire_declarations::MatchFacts;
use campfire_net_replication::Rooms;
use campfire_players::{OutOfPlay, Player, PlayersWorld};
use campfire_values::{PlayerView, Team, TeamSet};
use campfire_visibility::Recipients;
use campfire_visibility::{OutView, Visibility};

use crate::sim_server::team_rooms::TeamRooms;
use campfire_host::PlayerLink;

/// How a client link views the match, which every data filter reads (10-replication-audiences.md,
/// Four recipients; 10-replication-audiences.md, The host admits spectators, and their fog is a
/// setting; 10-replication-audiences.md, A player out of play sees what its team sees, or what a
/// spectator sees): as the player of a player slot of a team, or as a spectator. It stands in its
/// rooms by it, and in none without one. Immutable, as replicon's filters require: a change inserts
/// a new one.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable, on_insert = Viewer::enter_rooms, on_remove = Viewer::leave_rooms)]
pub(crate) enum Viewer {
    Player(PlayerView),
    Spectator(Watching),
}

/// What a spectator watches: every unit, through any fog, or the views of the teams it chose
/// together (10-replication-audiences.md, The host admits spectators, and their fog is a setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Watching {
    Everything,
    Teams(TeamSet),
}

/// Whether a player plays, or is out of play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayStatus {
    Playing,
    Out,
}

/// What viewing a player's link again reads: the players, the links, and the session's
/// visibility once the match started.
#[derive(SystemParam)]
pub(crate) struct PlayerViews<'w, 's> {
    players: Query<'w, 's, &'static Player>,
    links: Query<'w, 's, (Entity, &'static PlayerLink)>,
    facts: Option<Res<'w, MatchFacts>>,
    commands: Commands<'w, 's>,
}

/// Its queries and its commands print nothing: `Commands` has no `Debug`.
impl fmt::Debug for PlayerViews<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlayerViews").finish_non_exhaustive()
    }
}

impl PlayerViews<'_, '_> {
    /// Views again the link of the player whose entity is `player`, of `status`.
    fn view_again(&mut self, player: Entity, status: PlayStatus) {
        let Ok(player) = self.players.get(player) else {
            return;
        };
        let visibility = self.facts.as_deref().map(|facts| &facts.visibility);
        for (link, &player_link) in &self.links {
            if player_link.player_slot() == player.player_slot() {
                let viewer = Viewer::player(player_link, status, visibility);
                self.commands.entity(link).insert(viewer);
            }
        }
    }
}

impl Viewer {
    /// The view of the player of `link`, of `status`, in a session of `visibility`, none
    /// before the match starts: out as the session's `out` says.
    pub(crate) fn player(
        link: PlayerLink,
        status: PlayStatus,
        visibility: Option<&Visibility>,
    ) -> Self {
        let sees_as_spectator =
            visibility.is_some_and(|visibility| visibility.out() == OutView::Spectators);
        Self::Player(PlayerView {
            player_slot: link.player_slot(),
            team: link.team(),
            out: status == PlayStatus::Out && sees_as_spectator,
        })
    }

    /// The view of the player of `link` in `world`, as its entity stands.
    pub(crate) fn of_player(world: &World, link: PlayerLink) -> Self {
        let entity = world.player_entity(link.player_slot());
        let out = entity.is_some_and(|entity| world.entity(entity).contains::<OutOfPlay>());
        let status = if out {
            PlayStatus::Out
        } else {
            PlayStatus::Playing
        };
        let facts = world.get_resource::<MatchFacts>();
        Self::player(link, status, facts.map(|facts| &facts.visibility))
    }

    /// The recipient it is of an entity `owner` owns, of `team`: a player's, as its view says; a
    /// spectator.
    pub(crate) fn recipient(self, owner: Option<PlayerSlot>, team: Option<Team>) -> Recipients {
        match self {
            Self::Player(view) => view.recipient(owner, team),
            Self::Spectator(_) => Recipients::SPECTATORS,
        }
    }

    /// Views a link as its player as it gains a `PlayerLink`, or a new one, and as none once it
    /// loses it.
    pub(crate) fn follow_links(app: &mut App) {
        app.add_observer(
            |linked: On<'_, '_, Insert<PlayerLink>>, mut world: DeferredWorld<'_>| {
                let link = *world
                    .get::<PlayerLink>(linked.entity)
                    .expect("an insert's own component");
                let viewer = Self::of_player(&world, link);
                world.commands().entity(linked.entity).insert(viewer);
            },
        );
        app.add_observer(
            |unlinked: On<'_, '_, Remove<PlayerLink>>, mut commands: Commands<'_, '_>| {
                commands.entity(unlinked.entity).try_remove::<Self>();
            },
        );
    }

    /// Views each player's link again as its player goes out of play and comes back, which changes
    /// its rooms and what each data filter gives it (10-replication-audiences.md, A player out of
    /// play sees what its team sees, or what a spectator sees): what it held of the other teams'
    /// entities leaves it as a unit that leaves vision does.
    pub(crate) fn follow_out_of_play(app: &mut App) {
        app.add_observer(
            |out: On<'_, '_, Insert<OutOfPlay>>, mut views: PlayerViews<'_, '_>| {
                views.view_again(out.entity, PlayStatus::Out);
            },
        );
        app.add_observer(
            |back: On<'_, '_, Remove<OutOfPlay>>, mut views: PlayerViews<'_, '_>| {
                views.view_again(back.entity, PlayStatus::Playing);
            },
        );
    }

    fn enter_rooms(mut world: DeferredWorld<'_>, context: HookContext) {
        let viewer = *world
            .get::<Self>(context.entity)
            .expect("on_insert runs on an entity with a Viewer");
        let rooms = world.resource::<TeamRooms>();
        let facts = world.get_resource::<MatchFacts>();
        let rooms = match viewer {
            Self::Player(PlayerView {
                team, out: false, ..
            }) => rooms.of(team),
            Self::Player(PlayerView {
                team, out: true, ..
            }) => {
                let facts = facts.expect("a player is out of play in a running match");
                rooms.out_of_play(team, &facts.visibility)
            }
            Self::Spectator(Watching::Everything) => rooms.everything(),
            Self::Spectator(Watching::Teams(teams)) => rooms.watching(teams),
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
    use campfire_visibility::{HostVisibility, ModeVisibility};

    use super::*;

    #[test]
    fn a_viewer_is_the_owner_an_ally_an_enemy_or_a_spectator() {
        let (player_slot, team) = (PlayerSlot::new, Team::new);
        let link = PlayerLink::new(player_slot(1), team(0));
        let player = Viewer::player(link, PlayStatus::Playing, None);
        // Of a unit of player slot 1's: none, as the owner; of player slot 2's of its team, an
        // ally; of the other team's, an enemy; of the match's, of no team or owner, an ally.
        assert_eq!(
            player.recipient(Some(player_slot(1)), Some(team(0))),
            Recipients::NONE
        );
        assert_eq!(
            player.recipient(Some(player_slot(2)), Some(team(0))),
            Recipients::ALLIES
        );
        assert_eq!(player.recipient(None, Some(team(1))), Recipients::ENEMIES);
        assert_eq!(player.recipient(None, None), Recipients::ALLIES);
        let spectator = Viewer::Spectator(Watching::Teams(TeamSet::ALL));
        for (owner, of) in [
            (Some(player_slot(1)), Some(team(0))),
            (None, Some(team(1))),
            (None, None),
        ] {
            assert_eq!(spectator.recipient(owner, of), Recipients::SPECTATORS);
        }

        // Out of play, in a session whose players out of play see as spectators: a spectator of
        // the other team's units, and still the owner and an ally of its own. With `out = team`,
        // or before the match starts, out of play changes nothing; nor does play in either.
        let session = |out: &str| {
            let host = HostVisibility {
                settings: vec![format!("out={out}").parse().unwrap()],
                max_spectators: None,
            };
            ModeVisibility::default().session(&host).unwrap()
        };
        let (spectators, own_team) = (session("spectators"), session("team"));
        let out = Viewer::player(link, PlayStatus::Out, Some(&spectators));
        assert_eq!(out.recipient(None, Some(team(1))), Recipients::SPECTATORS);
        assert_eq!(
            out.recipient(Some(player_slot(1)), Some(team(0))),
            Recipients::NONE
        );
        assert_eq!(
            out.recipient(Some(player_slot(2)), Some(team(0))),
            Recipients::ALLIES
        );
        for (status, visibility) in [
            (PlayStatus::Out, Some(&own_team)),
            (PlayStatus::Out, None),
            (PlayStatus::Playing, Some(&spectators)),
        ] {
            let viewer = Viewer::player(link, status, visibility);
            assert_eq!(viewer, player, "{status:?} {visibility:?}");
        }
    }
}
