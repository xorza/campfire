use std::fmt;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_common::{ErrorReport, Tick};
use campfire_host::JoinError;
use campfire_host::Joined;
use campfire_host::JournalCommand;
use campfire_host::JournalRefusal;
use campfire_host::JournalRefused;
use campfire_host::MatchClock;
use campfire_host::PlayerSlots;
use campfire_host::Reports;
use campfire_host::WatchStart;
use campfire_log::LogEvent;
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_protocol::Delegation;
use campfire_runner::Session;
use campfire_values::TeamSet;
use tracing::{error, info};

use crate::sim_server::offering::{CheckedJoins, JoinLinks, Offering};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::viewer::{Viewer, Watching};

/// The door of a session that replays a published log (08-sessions.md, The client opens a
/// report): it offers the log's terms to every new link and admits one spectator, who watches the
/// views of the teams it chose, or every unit when it chose every team or none. A player finds no
/// player slot, as the log alone plays them, and the session logs nothing of either; a report
/// the spectator takes holds the log.
#[derive(Resource, Debug)]
pub(crate) struct ReplayDoor {
    offering: Offering,
}

/// The replay's places for a frame's joins: the spectator already watching, if one, and what an
/// admitted one is told.
struct ReplayPlaces<'a, 'w, 's, 'qw, 'qs> {
    commands: &'a mut Commands<'w, 's>,
    watch_starts: &'a mut Query<'qw, 'qs, &'static mut MessageSender<WatchStart>>,
    clock: &'a MatchClock,
    first: Tick,
    watching: usize,
}

impl AsRef<Offering> for ReplayDoor {
    fn as_ref(&self) -> &Offering {
        &self.offering
    }
}

impl ReplayDoor {
    pub(crate) const fn new(offering: Offering) -> Self {
        Self { offering }
    }

    /// Takes the journal commands of the spectator: a report's writes the log the replay replays
    /// into the report's folder, so the report replays as the one it opens; a save or a load is
    /// refused, as a replay keeps no save. Each is logged.
    pub(crate) fn take_commands(
        mut links: Query<'_, '_, &mut MessageReceiver<JournalCommand>, With<Viewer>>,
        session: Res<'_, Session>,
        reports: Option<Res<'_, Reports>>,
    ) {
        for mut receiver in &mut links {
            for command in receiver.receive() {
                let (JournalCommand::Report(key), Some(reports)) = (command, reports.as_deref())
                else {
                    JournalRefused {
                        refusal: JournalRefusal::Replay,
                    }
                    .log();
                    continue;
                };
                match reports.write_replayed(key, session.log()) {
                    Ok(()) => info!(tick = %key.tick, "wrote the replayed log into the report"),
                    Err(problem) => {
                        error!(tick = %key.tick, error = %ErrorReport::of(&problem), "could not write the replayed log into the report");
                    }
                }
            }
        }
    }

    /// Admits the spectator of each offered link that answered to watch while none watches, and
    /// refuses every other; the joins of one frame in `Offering::check_all`'s order.
    pub(crate) fn take_joins(
        mut door: ResMut<'_, Self>,
        mut links: JoinLinks<'_, '_>,
        viewers: Query<'_, '_, &Viewer, With<Joined>>,
        (clock, session): (Res<'_, MatchClock>, Res<'_, Session>),
        mut watch_starts: Query<'_, '_, &'static mut MessageSender<WatchStart>>,
        mut commands: Commands<'_, '_>,
        mut checked: Local<'_, CheckedJoins>,
    ) {
        let joins = links.iter_mut().filter_map(Offering::received);
        door.offering.check_all(joins, &mut checked);
        checked.admit(&mut ReplayPlaces {
            commands: &mut commands,
            watch_starts: &mut watch_starts,
            clock: &clock,
            first: session.next_tick(),
            watching: Spectators::count(viewers.iter()),
        });
    }
}

/// The places by their count of spectators; the system params they borrow show nothing more.
impl fmt::Debug for ReplayPlaces<'_, '_, '_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplayPlaces")
            .field("first", &self.first)
            .field("watching", &self.watching)
            .finish_non_exhaustive()
    }
}

impl<'w, 's> PlayerSlots<'w, 's> for ReplayPlaces<'_, 'w, 's, '_, '_> {
    fn commands(&mut self) -> &mut Commands<'w, 's> {
        self.commands
    }

    /// Admits the spectator while none watches, and tells it the match's clock from the next
    /// tick on.
    fn watch(&mut self, link: Entity, teams: TeamSet) -> Result<(), JoinError> {
        if self.watching > 0 {
            return Err(JoinError::NoSpectatorPlace);
        }
        let watching = if teams == TeamSet::ALL || teams == TeamSet::NONE {
            Watching::Everything
        } else {
            Watching::Teams(teams)
        };
        self.commands
            .entity(link)
            .insert((Joined, Viewer::Spectator(watching)));
        self.watching += 1;
        let mut sender = self
            .watch_starts
            .get_mut(link)
            .expect("a client link sends the watch start");
        Spectators::tell_start(&mut sender, self.clock, self.first, link);
        Ok(())
    }

    fn player_slot(&mut self, _: Entity, _: Delegation) -> Result<(), JoinError> {
        Err(JoinError::NoPlayerSlot)
    }
}
