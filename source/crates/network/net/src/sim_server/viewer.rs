use bevy_app::App;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Insert, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::system::{Commands, Query, Res};
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

impl Viewer {
    /// The view of the player of `slot`, of `team`, out of play when `out_of_play`, in a session
    /// of `visibility`, none before the match starts: out as the session's `out` says.
    pub(crate) fn player(
        slot: PlayerSlot,
        team: Team,
        out_of_play: bool,
        visibility: Option<&Visibility>,
    ) -> Viewer {
        let sees_as_spectator =
            visibility.is_some_and(|visibility| visibility.out() == OutView::Spectators);
        Viewer::Player {
            slot,
            team,
            out: out_of_play && sees_as_spectator,
        }
    }

    /// The view of the player of `slot`, of `team`, in `world`, as its entity stands.
    pub(crate) fn of_player(world: &World, slot: PlayerSlot, team: Team) -> Viewer {
        let entity = world.player_entity(slot);
        let out_of_play = entity.is_some_and(|entity| world.entity(entity).contains::<OutOfPlay>());
        let visibility = world.get_resource::<MatchVisibility>();
        Viewer::player(
            slot,
            team,
            out_of_play,
            visibility.map(|visibility| &visibility.0),
        )
    }

    /// The recipient it is of an entity `owner` owns, of `team`: none for the owner, who receives
    /// every sent kind; an ally of its own team, or of an entity of no team, as the match's;
    /// an enemy of another, or a spectator of it while out of play; a spectator.
    pub(crate) fn recipient(self, owner: Option<PlayerSlot>, team: Option<Team>) -> Recipients {
        match self {
            Viewer::Player { slot, .. } if Some(slot) == owner => Recipients::NONE,
            Viewer::Player { team: own, out, .. } => match team {
                Some(team) if team != own && out => Recipients::SPECTATORS,
                Some(team) if team != own => Recipients::ENEMIES,
                _ => Recipients::ALLIES,
            },
            Viewer::Spectator(_) => Recipients::SPECTATORS,
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
                let viewer = Viewer::of_player(&world, link.slot(), link.team());
                world.commands().entity(linked.entity).insert(viewer);
            },
        );
        app.add_observer(
            |unlinked: On<'_, '_, Remove, PlayerLink>, mut commands: Commands<'_, '_>| {
                commands.entity(unlinked.entity).try_remove::<Viewer>();
            },
        );
    }

    /// Views each player's link again as its player goes out of play and comes back, which
    /// changes its rooms and what each data filter gives it (design 14, D16): what it held of
    /// the other teams' entities leaves it as a unit that leaves vision does.
    pub(crate) fn follow_out_of_play(app: &mut App) {
        app.add_observer(
            |out: On<'_, '_, Insert, OutOfPlay>,
             players: Query<'_, '_, &Player>,
             links: Query<'_, '_, (Entity, &PlayerLink)>,
             visibility: Option<Res<'_, MatchVisibility>>,
             commands: Commands<'_, '_>| {
                let visibility = visibility.as_deref().map(|visibility| &visibility.0);
                Viewer::view_again(out.entity, true, &players, &links, visibility, commands);
            },
        );
        app.add_observer(
            |back: On<'_, '_, Remove, OutOfPlay>,
             players: Query<'_, '_, &Player>,
             links: Query<'_, '_, (Entity, &PlayerLink)>,
             visibility: Option<Res<'_, MatchVisibility>>,
             commands: Commands<'_, '_>| {
                let visibility = visibility.as_deref().map(|visibility| &visibility.0);
                Viewer::view_again(back.entity, false, &players, &links, visibility, commands);
            },
        );
    }

    /// Views again the link of the player whose entity is `player`, out of play as `out_of_play`.
    fn view_again(
        player: Entity,
        out_of_play: bool,
        players: &Query<'_, '_, &Player>,
        links: &Query<'_, '_, (Entity, &PlayerLink)>,
        visibility: Option<&Visibility>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok(player) = players.get(player) else {
            return;
        };
        for (link, seat) in links {
            if seat.slot() == player.slot() {
                let viewer = Viewer::player(seat.slot(), seat.team(), out_of_play, visibility);
                commands.entity(link).insert(viewer);
            }
        }
    }

    fn enter_rooms(mut world: DeferredWorld<'_>, context: HookContext) {
        let viewer = *world
            .get::<Viewer>(context.entity)
            .expect("on_insert runs on an entity with a Viewer");
        let rooms = world.resource::<TeamRooms>();
        let rooms = match viewer {
            Viewer::Player {
                team, out: false, ..
            } => rooms.of(team),
            Viewer::Player {
                team, out: true, ..
            } => rooms.out_of_play(team),
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
    use campfire_visibility::{HostVisibility, ModeVisibility};

    use super::*;

    #[test]
    fn a_viewer_is_the_owner_an_ally_an_enemy_or_a_spectator() {
        let (slot, team) = (PlayerSlot::new, Team::new);
        let player = Viewer::player(slot(1), team(0), false, None);
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
        let out = Viewer::player(slot(1), team(0), true, Some(&spectators));
        assert_eq!(out.recipient(None, Some(team(1))), Recipients::SPECTATORS);
        assert_eq!(
            out.recipient(Some(slot(1)), Some(team(0))),
            Recipients::NONE
        );
        assert_eq!(
            out.recipient(Some(slot(2)), Some(team(0))),
            Recipients::ALLIES
        );
        for (out_of_play, visibility) in [
            (true, Some(&own_team)),
            (true, None),
            (false, Some(&spectators)),
        ] {
            let viewer = Viewer::player(slot(1), team(0), out_of_play, visibility);
            assert_eq!(viewer, player, "{out_of_play} {visibility:?}");
        }
    }
}
