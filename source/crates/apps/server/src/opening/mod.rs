use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use campfire_common::MapName;
use campfire_net::{
    Lobby, LobbyOpening, PendingRestore, Publisher, RestoredSession, ServerBots, ServerDir,
    ServerSetup, SessionDir,
};
use campfire_package::ModePackages;
use campfire_protocol::SessionTerms;
use campfire_runner::SessionRules;
use campfire_visibility::HostVisibility;

use crate::opening::error::OpeningError;

pub(crate) mod error;

/// What a server starts with: its session, whose terms the mode's rules admit, and the seats
/// of that session.
#[derive(Debug)]
pub(crate) struct Opening {
    pub(crate) session: Opened,
    seats: NonZeroU32,
}

/// The session a server starts: a new one, open to its players, or the one a stop ended, to
/// restore. A session past the restore window, or one that ended, has its log published
/// instead, and a new one opens.
#[derive(Debug)]
pub(crate) enum Opened {
    New(Box<Lobby>),
    Restored(Box<PendingRestore>),
}

/// What `Opening::of` needs.
#[derive(Debug)]
pub(crate) struct OpeningSetup<'a> {
    pub(crate) data: &'a ServerDir,
    pub(crate) packages: ModePackages,
    pub(crate) server: ServerSetup,
    pub(crate) bots: ServerBots,
    /// What the host sets of a new session's visibility; a restored session keeps its own.
    pub(crate) host: HostVisibility,
}

impl Opening {
    /// What the server starts: `found`, the session to restore that `find` gave, or a new one,
    /// its directory, private record and journal made. An error for a session or a journal not
    /// made, and for a restored session whose terms the mode's rules do not admit, as a journal
    /// another release wrote holds.
    pub(crate) fn of(
        found: Option<RestoredSession>,
        setup: OpeningSetup<'_>,
    ) -> Result<Self, OpeningError> {
        let rules = SessionRules::of(&setup.packages);
        let session = match found {
            Some(session) => Opened::Restored(Box::new(PendingRestore {
                session,
                packages: Arc::new(setup.packages),
                server: setup.server,
                bots: setup.bots,
            })),
            None => Opened::New(Box::new(Self::open(setup)?)),
        };
        let terms = session.terms();
        let visibility = rules.check(terms).map_err(OpeningError::Terms)?;
        let slots = u32::try_from(terms.slots.len()).expect("a session's slots fit u32");
        let seats = slots
            .checked_add(visibility.max_spectators())
            .expect("a session's seats fit u32");
        let seats = NonZeroU32::new(seats).expect("the rules admit a session of a slot at least");
        Ok(Self { session, seats })
    }

    /// The terms of the session it starts.
    pub(crate) fn terms(&self) -> &SessionTerms {
        self.session.terms()
    }

    /// The setup of the server that starts it.
    pub(crate) fn server(&self) -> &ServerSetup {
        match &self.session {
            Opened::New(lobby) => lobby.server(),
            Opened::Restored(restore) => &restore.server,
        }
    }

    /// The seats of the session it starts: its slots and the most spectators it admits, which no
    /// address of honest players passes in live connections (design 15, D13).
    pub(crate) const fn seats(&self) -> NonZeroU32 {
        self.seats
    }

    /// The session a stop ended, under the data directory `data`: none when the directory holds
    /// none, and when its match never started, whose directory goes. An error for a session that
    /// does not read.
    pub(crate) fn waiting(data: &ServerDir) -> Result<Option<RestoredSession>, OpeningError> {
        SessionDir::waiting(data).map_err(OpeningError::Waiting)
    }

    /// The map the server loads: `journal`, the one a waiting session's terms name, which it
    /// restores or ends, else the one `asked` names, else none, for the mode's only map. An
    /// error when a waiting session's map is not the one asked.
    pub(crate) fn map(
        journal: Option<&MapName>,
        asked: Option<MapName>,
    ) -> Result<Option<MapName>, OpeningError> {
        let Some(journal) = journal else {
            return Ok(asked);
        };
        match asked {
            Some(asked) if &asked != journal => Err(OpeningError::OtherMap {
                asked,
                journal: journal.clone(),
            }),
            _ => Ok(Some(journal.clone())),
        }
    }

    /// The session `waiting` to restore within the restore window `window`: when it is past the
    /// window or ended, `publisher` publishes its log instead, and none restores. An error for a
    /// session that does not end.
    pub(crate) fn find(
        waiting: Option<RestoredSession>,
        window: Duration,
        publisher: Publisher<'_>,
    ) -> Result<Option<RestoredSession>, OpeningError> {
        let Some(session) = waiting else {
            return Ok(None);
        };
        let idle = SystemTime::now()
            .duration_since(session.modified)
            .unwrap_or_default();
        if session.log.result().is_none() && idle <= window {
            return Ok(Some(session));
        }
        session.abort(publisher).map_err(OpeningError::Abort)?;
        Ok(None)
    }

    /// A new session of `setup.packages`, its directory, private record and journal made.
    fn open(setup: OpeningSetup<'_>) -> Result<Lobby, OpeningError> {
        let opening = LobbyOpening {
            packages: Arc::new(setup.packages),
            bots: setup.bots,
            open: Vec::new(),
            host: setup.host,
            server: setup.server,
        };
        Lobby::open(opening, setup.data).map_err(OpeningError::Open)
    }
}

impl Opened {
    /// The session's terms.
    fn terms(&self) -> &SessionTerms {
        match self {
            Self::New(lobby) => lobby.terms(),
            Self::Restored(restore) => &restore.session.log.header().terms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_sessions_map_is_loaded_and_another_asked_is_refused() {
        let name = |name: &str| MapName::new(name).unwrap();
        let (lane, river) = (name("lane"), name("river"));
        // No session waits: the map asked, or none, for the mode's only map.
        assert_eq!(
            Opening::map(None, Some(river.clone())).unwrap(),
            Some(river.clone())
        );
        assert_eq!(Opening::map(None, None).unwrap(), None);
        // A session of `lane` waits: `lane`, asked or not; `river` asked is refused, naming both.
        assert_eq!(Opening::map(Some(&lane), None).unwrap(), Some(lane.clone()));
        assert_eq!(
            Opening::map(Some(&lane), Some(lane.clone())).unwrap(),
            Some(lane.clone())
        );
        assert!(matches!(
            Opening::map(Some(&lane), Some(river.clone())),
            Err(OpeningError::OtherMap { asked, journal }) if asked == river && journal == lane
        ));
    }
}
