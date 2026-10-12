use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::World;
use campfire_capabilities::{SpectatorFog, TeamSet, Visibility};
use campfire_runner::Session;
use lightyear::prelude::{Connected, MessageReceiver, MessageSender};
use tracing::info;

use crate::match_clock::MatchClock;
use crate::net_protocol::MatchChannel;
use crate::sim_server::error::JoinError;
use crate::sim_server::offering::Joined;
use crate::sim_server::viewer::{Viewer, Watching};
use crate::watch_start::WatchStart;
use crate::watch_teams::WatchTeams;

/// The links that watch the session with no slot (design 14, D15): each joins by a signed
/// answer, as a player does, takes a place while the session admits one more, and watches as its
/// view says. The log holds nothing of them.
#[derive(Debug)]
pub(crate) struct Spectators;

impl Spectators {
    /// The view of a spectator that joins a session of `visibility` with `watching` spectators
    /// in it: every unit, or every team's view together, which it may change; an error when every
    /// place the session admits is taken.
    pub(crate) fn place(visibility: &Visibility, watching: usize) -> Result<Viewer, JoinError> {
        let watching = u32::try_from(watching).unwrap_or(u32::MAX);
        if watching >= visibility.max_spectators() {
            return Err(JoinError::NoSpectatorPlace);
        }
        Ok(Viewer::Spectator(match visibility.spectators() {
            SpectatorFog::All => Watching::Everything,
            SpectatorFog::Teams => Watching::Teams(TeamSet::ALL),
        }))
    }

    /// How many links of `viewers` watch.
    pub(crate) fn count<'v>(viewers: impl Iterator<Item = &'v Viewer>) -> usize {
        viewers
            .filter(|viewer| matches!(viewer, Viewer::Spectator(_)))
            .count()
    }

    /// Admits `link` to watch the match `world` runs, a session of `visibility`, and tells its
    /// client the match's tick; an error when every place is taken.
    pub(crate) fn admit(
        world: &mut World,
        link: Entity,
        visibility: &Visibility,
    ) -> Result<(), JoinError> {
        let mut viewers = world.query::<&Viewer>();
        let viewer = Spectators::place(visibility, Spectators::count(viewers.iter(world)))?;
        world.entity_mut(link).insert((Joined, viewer));
        Spectators::start(world, link);
        Ok(())
    }

    /// Tells the client of `link`, a spectator, the Lightyear tick of the match's next sim tick.
    pub(crate) fn start(world: &mut World, link: Entity) {
        let first = world.resource::<Session>().log().next_tick();
        let start_tick = world.resource::<MatchClock>().net_tick(first);
        world
            .get_mut::<MessageSender<WatchStart>>(link)
            .expect("a client link sends the watch start")
            .send::<MatchChannel>(WatchStart {
                start_tick: start_tick.0,
                first,
            });
        info!(?link, "a spectator watches");
    }

    /// Tells each spectator's client the match's tick, as the match starts.
    pub(crate) fn start_every(world: &mut World) {
        let mut viewers = world.query::<(Entity, &Viewer)>();
        let spectators: Vec<Entity> = viewers
            .iter(world)
            .filter(|(_, viewer)| matches!(viewer, Viewer::Spectator(_)))
            .map(|(link, _)| link)
            .collect();
        for link in spectators {
            Spectators::start(world, link);
        }
    }

    /// Ends the watch of a spectator whose link is no longer connected, so it holds no place
    /// and stands in no room: a link that fails can stay as an entity, and a spectator that
    /// joins again comes on a new one.
    pub(crate) fn leave(
        removed: On<'_, '_, Remove, Connected>,
        viewers: Query<'_, '_, &Viewer>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(Viewer::Spectator(_)) = viewers.get(removed.entity) {
            commands.entity(removed.entity).try_remove::<Viewer>();
            info!(link = ?removed.entity, "a spectator's link ended");
        }
    }

    /// Takes each spectator's choice of teams: one that watches teams' views watches those it
    /// names, at least one; one that sees every unit keeps it.
    pub(crate) fn choose_teams(
        mut links: Query<'_, '_, (Entity, &Viewer, &mut MessageReceiver<WatchTeams>)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (link, &viewer, mut receiver) in &mut links {
            let Some(WatchTeams(teams)) = receiver.receive().last() else {
                continue;
            };
            if matches!(viewer, Viewer::Spectator(Watching::Teams(_))) && teams != TeamSet::NONE {
                let watching = Viewer::Spectator(Watching::Teams(teams));
                commands.entity(link).insert(watching);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_capabilities::{HostVisibility, ModeVisibility};

    use super::*;

    fn visibility(fog: &str, max_spectators: u32) -> Visibility {
        let host = HostVisibility {
            settings: vec![format!("spectators={fog}").parse().unwrap()],
            max_spectators,
        };
        ModeVisibility::default().session(&host).unwrap()
    }

    #[test]
    fn a_spectator_takes_a_place_while_the_session_admits_one_and_watches_as_its_fog_says() {
        // Two places: the first two spectators watch, the third finds none; none in a session
        // that admits none.
        let all = visibility("all", 2);
        let everything = Ok(Viewer::Spectator(Watching::Everything));
        assert_eq!(Spectators::place(&all, 0), everything);
        assert_eq!(Spectators::place(&all, 1), everything);
        assert_eq!(Spectators::place(&all, 2), Err(JoinError::NoSpectatorPlace));
        assert_eq!(
            Spectators::place(&Visibility::DEFAULT, 0),
            Err(JoinError::NoSpectatorPlace)
        );
        // The teams' views: every team's together, until it chooses.
        assert_eq!(
            Spectators::place(&visibility("teams", 1), 0),
            Ok(Viewer::Spectator(Watching::Teams(TeamSet::ALL)))
        );
    }
}
