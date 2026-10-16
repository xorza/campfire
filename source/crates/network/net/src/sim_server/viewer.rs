use bevy_app::App;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::system::{Commands, Query, Res, SystemParam};
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::PlayerSlot;
use campfire_players::{OutOfPlay, PlayersWorld};
use campfire_state_types::Recipients;
use campfire_units::Player;
use campfire_values::{Team, TeamSet};
use campfire_visibility::{OutView, Visibility};
use lightyear::prelude::Rooms;

use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::team_rooms::TeamRooms;

/// How a client link views the match, which every data filter reads (design 14, D1, D15, D16):
/// as the player of a slot of a team, or as a spectator. It stands in its rooms by it, and in none
/// without one. Immutable, as replicon's filters require: a change inserts a new one.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable, on_insert = Viewer::enter_rooms, on_remove = Viewer::leave_rooms)]
pub(crate) enum Viewer {
    /// The player of `slot`, of `team`; `out` while it is out of play in a session whose players
    /// out of play see the other teams as a spectator does.
    Player {
        slot: PlayerSlot,
        team: Team,
        out: bool,
    },
    Spectator(Watching),
}

/// What a spectator watches: every unit, through any fog, or the views of the teams it chose
/// together (design 14, D15).
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
    visibility: Option<Res<'w, MatchVisibility>>,
    commands: Commands<'w, 's>,
}

impl PlayerViews<'_, '_> {
    /// Views again the link of the player whose entity is `player`, of `status`.
    fn view_again(&mut self, player: Entity, status: PlayStatus) {
        let Ok(player) = self.players.get(player) else {
            return;
        };
        let visibility = self.visibility.as_deref().map(|visibility| &visibility.0);
        for (link, &seat) in &self.links {
            if seat.slot() == player.slot() {
                let viewer = Viewer::player(seat, status, visibility);
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
        Self::Player {
            slot: link.slot(),
            team: link.team(),
            out: status == PlayStatus::Out && sees_as_spectator,
        }
    }

    /// The view of the player of `link` in `world`, as its entity stands.
    pub(crate) fn of_player(world: &World, link: PlayerLink) -> Self {
        let entity = world.player_entity(link.slot());
        let out = entity.is_some_and(|entity| world.entity(entity).contains::<OutOfPlay>());
        let status = if out {
            PlayStatus::Out
        } else {
            PlayStatus::Playing
        };
        let visibility = world.get_resource::<MatchVisibility>();
        Self::player(link, status, visibility.map(|visibility| &visibility.0))
    }

    /// The recipient it is of an entity `owner` owns, of `team`: none for the owner, who receives
    /// every sent kind; an ally of its own team, or of an entity of no team, as the match's;
    /// an enemy of another, or a spectator of it while out of play; a spectator.
    pub(crate) fn recipient(self, owner: Option<PlayerSlot>, team: Option<Team>) -> Recipients {
        match self {
            Self::Player { slot, .. } if Some(slot) == owner => Recipients::NONE,
            Self::Player { team: own, out, .. } => match team {
                Some(team) if team != own && out => Recipients::SPECTATORS,
                Some(team) if team != own => Recipients::ENEMIES,
                _ => Recipients::ALLIES,
            },
            Self::Spectator(_) => Recipients::SPECTATORS,
        }
    }

    /// Views a link as its player as it gains a `PlayerLink`, or a new one, and as none once it
    /// loses it.
    pub(crate) fn follow_links(app: &mut App) {
        app.add_observer(
            |linked: On<'_, '_, Insert, PlayerLink>, mut world: DeferredWorld<'_>| {
                let link = *world
                    .get::<PlayerLink>(linked.entity)
                    .expect("an insert's own component");
                let viewer = Self::of_player(&world, link);
                world.commands().entity(linked.entity).insert(viewer);
            },
        );
        app.add_observer(
            |unlinked: On<'_, '_, Remove, PlayerLink>, mut commands: Commands<'_, '_>| {
                commands.entity(unlinked.entity).try_remove::<Self>();
            },
        );
    }

    /// Views each player's link again as its player goes out of play and comes back, which
    /// changes its rooms and what each data filter gives it (design 14, D16): what it held of
    /// the other teams' entities leaves it as a unit that leaves vision does.
    pub(crate) fn follow_out_of_play(app: &mut App) {
        app.add_observer(
            |out: On<'_, '_, Insert, OutOfPlay>, mut views: PlayerViews<'_, '_>| {
                views.view_again(out.entity, PlayStatus::Out);
            },
        );
        app.add_observer(
            |back: On<'_, '_, Remove, OutOfPlay>, mut views: PlayerViews<'_, '_>| {
                views.view_again(back.entity, PlayStatus::Playing);
            },
        );
    }

    fn enter_rooms(mut world: DeferredWorld<'_>, context: HookContext) {
        let viewer = *world
            .get::<Self>(context.entity)
            .expect("on_insert runs on an entity with a Viewer");
        let rooms = world.resource::<TeamRooms>();
        let visibility = world.get_resource::<MatchVisibility>();
        let rooms = match viewer {
            Self::Player {
                team, out: false, ..
            } => rooms.of(team),
            Self::Player {
                team, out: true, ..
            } => {
                let visibility = visibility.expect("a player is out of play in a running match");
                rooms.out_of_play(team, &visibility.0)
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
        let (slot, team) = (PlayerSlot::new, Team::new);
        let seat = PlayerLink::new(slot(1), team(0));
        let player = Viewer::player(seat, PlayStatus::Playing, None);
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

        // Out of play, in a session whose players out of play see as spectators: a spectator of
        // the other team's units, and still the owner and an ally of its own. With `out = team`,
        // or before the match starts, out of play changes nothing; nor does play in either.
        let session = |out: &str| {
            let host = HostVisibility {
                settings: vec![format!("out={out}").parse().unwrap()],
                max_spectators: 0,
            };
            ModeVisibility::default().session(&host).unwrap()
        };
        let (spectators, own_team) = (session("spectators"), session("team"));
        let out = Viewer::player(seat, PlayStatus::Out, Some(&spectators));
        assert_eq!(out.recipient(None, Some(team(1))), Recipients::SPECTATORS);
        assert_eq!(
            out.recipient(Some(slot(1)), Some(team(0))),
            Recipients::NONE
        );
        assert_eq!(
            out.recipient(Some(slot(2)), Some(team(0))),
            Recipients::ALLIES
        );
        for (status, visibility) in [
            (PlayStatus::Out, Some(&own_team)),
            (PlayStatus::Out, None),
            (PlayStatus::Playing, Some(&spectators)),
        ] {
            let viewer = Viewer::player(seat, status, visibility);
            assert_eq!(viewer, player, "{status:?} {visibility:?}");
        }
    }
}
