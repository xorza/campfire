use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::system::{Commands, Query};
use campfire_common::Tick;
use campfire_net_connection::Connected;
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_values::TeamSet;
use campfire_visibility::{SpectatorFog, Visibility};
use tracing::info;

use crate::error::JoinError;
use crate::match_clock::MatchClock;
use crate::net_protocol::MatchChannel;
use crate::net_protocol::watch_start::WatchStart;
use crate::net_protocol::watch_teams::WatchTeams;
use crate::sim_server::viewer::{Viewer, Watching};

pub(crate) mod spectators_world;

/// The links that watch the session with no seat (10-replication-audiences.md, The host admits
/// spectators, and their fog is a setting): each joins by a signed answer, as a player does, takes
/// a place while the session admits one more, and watches as its view says. The log holds nothing
/// of them.
#[derive(Debug)]
pub(crate) struct Spectators;

impl Spectators {
    /// Tells the client of `link`, a spectator, through `sender`, the Lightyear tick of the
    /// match's next sim tick, `first`, by `clock`.
    pub(crate) fn tell_start(
        sender: &mut MessageSender<WatchStart>,
        clock: &MatchClock,
        first: Tick,
        link: Entity,
    ) {
        let start_tick = clock.net_tick(first);
        sender.send::<MatchChannel>(WatchStart {
            start_tick: start_tick.0,
            first,
        });
        info!(link = link.to_bits(), "a spectator watches");
    }

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
    pub(crate) fn count<'v>(viewers: impl IntoIterator<Item = &'v Viewer>) -> usize {
        viewers
            .into_iter()
            .filter(|viewer| matches!(viewer, Viewer::Spectator(_)))
            .count()
    }

    /// Ends the watch of a spectator whose link is no longer connected, so it holds no place
    /// and stands in no room: a link that fails can stay as an entity, and a spectator that
    /// joins again comes on a new one.
    pub(crate) fn leave(
        removed: On<'_, '_, Remove<Connected>>,
        viewers: Query<'_, '_, &Viewer>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(Viewer::Spectator(_)) = viewers.get(removed.entity) {
            commands.entity(removed.entity).try_remove::<Viewer>();
            info!(link = removed.entity.to_bits(), "a spectator's link ended");
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
    use campfire_visibility::{HostVisibility, ModeVisibility};

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
