use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use campfire_common::MapName;
use campfire_mode::{OptionPicks, OptionValue};
use campfire_net::{
    Lobby, LobbyOpening, PendingRestore, Publisher, RestoredSession, ServerBots, ServerDir,
    ServerSetup, SessionDir, UncheckedRestore,
};
use campfire_package::ModePackages;
use campfire_protocol::SessionTerms;
use campfire_runner::SessionRules;
use campfire_visibility::{HostVisibility, Visibility};

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
    /// What the host sets of a new session's visibility; a restored session keeps its own, and
    /// refuses another.
    pub(crate) host: HostVisibility,
    /// The values the host picks of a new session's options; a restored session keeps its own.
    pub(crate) options: Vec<OptionValue>,
}

/// How long a session waits to restore, and the time of the start that asks: a session whose
/// journal was written within `length` of `now` restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RestoreWindow {
    pub(crate) length: Duration,
    pub(crate) now: SystemTime,
}

impl Opening {
    /// What the server starts: `found`, the session to restore that `find` gave, or a new one,
    /// its directory, private record and journal made, its terms checked once. An error for a
    /// session or a journal not made, for a restored session whose terms the mode's rules do not
    /// admit, and for one of other picks or another visibility than the host asks.
    pub(crate) fn of(
        found: Option<RestoredSession>,
        setup: OpeningSetup<'_>,
    ) -> Result<Self, OpeningError> {
        let (session, visibility) = if let Some(session) = found {
            let restore = PendingRestore::check(UncheckedRestore {
                session,
                packages: Arc::new(setup.packages),
                server: setup.server,
                bots: setup.bots,
            })
            .map_err(OpeningError::Terms)?;
            let visibility = restore.checked().visibility;
            let rules = SessionRules::of(&restore.packages);
            Self::picks(rules, &restore.session.terms().options, &setup.options)?;
            Self::visibility(rules, &visibility, &setup.host)?;
            (Opened::Restored(Box::new(restore)), visibility)
        } else {
            let lobby = Self::open(setup)?;
            let visibility = lobby.checked().visibility;
            (Opened::New(Box::new(lobby)), visibility)
        };
        let terms = session.terms();
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
    /// address of honest players passes in live connections (15-quic-transport.md, Limits for each
    /// address, from the session's seats).
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

    /// An error when the host `asked` picks of the mode's options of `rules`, and they are not
    /// `journal`'s, the picks a waiting session's terms hold, which it restores with, as its map
    /// is checked (10-sessions.md, A pick is given by its value).
    fn picks(
        rules: SessionRules<'_>,
        journal: &[u8],
        asked: &[OptionValue],
    ) -> Result<(), OpeningError> {
        if asked.is_empty() {
            return Ok(());
        }
        let asked = OptionPicks::named(rules.options(), asked).map_err(OpeningError::Options)?;
        if asked.places() != journal {
            return Err(OpeningError::OtherPicks);
        }
        Ok(())
    }

    /// An error when the host `asked` settings of the visibility the mode leaves it, or a most of
    /// spectators, and they do not give `journal`, the visibility a waiting session's terms hold,
    /// which it restores with, as its picks are checked.
    fn visibility(
        rules: SessionRules<'_>,
        journal: &Visibility,
        asked: &HostVisibility,
    ) -> Result<(), OpeningError> {
        if *asked == HostVisibility::default() {
            return Ok(());
        }
        let asked = rules
            .visibility()
            .session(asked)
            .map_err(OpeningError::Visibility)?;
        if &asked != journal {
            return Err(OpeningError::OtherVisibility);
        }
        Ok(())
    }

    /// The session `waiting` to restore within `window`: when it is past the window or ended,
    /// `publisher` publishes its log instead, and none restores. An error for a session that
    /// does not end.
    pub(crate) fn find(
        waiting: Option<RestoredSession>,
        window: RestoreWindow,
        publisher: Publisher<'_>,
    ) -> Result<Option<RestoredSession>, OpeningError> {
        let Some(session) = waiting else {
            return Ok(None);
        };
        if session.log.result().is_none() && window.holds(session.modified) {
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
            options: setup.options,
            server: setup.server,
        };
        Lobby::open(opening, setup.data).map_err(OpeningError::Open)
    }
}

impl RestoreWindow {
    /// Whether a journal written at `modified` is within it; one written after `now`, as a
    /// clock set back gives, is.
    fn holds(self, modified: SystemTime) -> bool {
        let idle = self.now.duration_since(modified).unwrap_or_default();
        idle <= self.length
    }
}

impl Opened {
    /// The session's terms.
    fn terms(&self) -> &SessionTerms {
        match self {
            Self::New(lobby) => lobby.terms(),
            Self::Restored(restore) => restore.session.terms(),
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_test_packages::TestMode;

    use super::*;

    #[test]
    fn a_waiting_sessions_map_is_loaded_and_another_asked_is_refused() {
        let name = |name: &str| MapName::parse(name).unwrap();
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

    #[test]
    fn a_waiting_sessions_picks_are_kept_and_others_asked_are_refused() {
        let packages = TestMode::Skirmish.packages();
        let rules = SessionRules::of(&packages);
        let asked = |values: &[&str]| -> Vec<OptionValue> {
            values.iter().map(|value| value.parse().unwrap()).collect()
        };
        // A session of the skirmish's 5,000, its second value, waits: no pick asked, and its
        // own, restore it; the default asked, a value of none, and a value it lacks, are refused.
        assert!(Opening::picks(rules, &[1], &[]).is_ok());
        assert!(Opening::picks(rules, &[1], &asked(&["start_money=5000"])).is_ok());
        assert!(matches!(
            Opening::picks(rules, &[1], &asked(&["start_money=100"])),
            Err(OpeningError::OtherPicks)
        ));
        assert!(matches!(
            Opening::picks(rules, &[1], &asked(&["start_money=7"])),
            Err(OpeningError::Options(_))
        ));
    }

    #[test]
    fn a_waiting_sessions_visibility_is_kept_and_another_asked_is_refused() {
        let packages = TestMode::Skirmish.packages();
        let rules = SessionRules::of(&packages);
        let hosted = |settings: &[&str], max_spectators| HostVisibility {
            settings: settings
                .iter()
                .map(|value| value.parse().unwrap())
                .collect(),
            max_spectators,
        };
        let session = |host: &HostVisibility| rules.visibility().session(host).unwrap();
        // A session of two spectators who see all waits: nothing asked, and its own, restore it;
        // another most of spectators, or another fog of theirs, is refused.
        let own = hosted(&["spectators=all"], 2);
        let journal = session(&own);
        assert!(Opening::visibility(rules, &journal, &HostVisibility::default()).is_ok());
        assert!(Opening::visibility(rules, &journal, &own).is_ok());
        for other in [
            hosted(&["spectators=all"], 3),
            hosted(&["spectators=teams"], 2),
        ] {
            assert!(matches!(
                Opening::visibility(rules, &journal, &other),
                Err(OpeningError::OtherVisibility)
            ));
        }
        // A setting the mode does not leave the host is refused as the visibility asked.
        assert!(matches!(
            Opening::visibility(rules, &journal, &hosted(&["inventory=allies"], 2)),
            Err(OpeningError::Visibility(_))
        ));
    }
}
