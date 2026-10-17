use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_time::{Real, Time, Virtual};
use campfire_common::PlayerSlot;
use campfire_package::ModePackages;
use campfire_protocol::{ServerInput, ServerSeeds, SessionLog, SessionTerms};
use campfire_runner::{CheckedTerms, Session, SessionSource};
use campfire_sim::TickRate;
use campfire_visibility::Visibility;
use lightyear::core::tick::TickDuration;
use lightyear::prelude::LocalTimeline;

use crate::match_clock::MatchClock;
use crate::pace::PaceSpeed;
use crate::sim_server::bot_driver::BotDriver;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::door::Door;
use crate::sim_server::error::MatchBeginError;
use crate::sim_server::journal_watch::{JournalFrom, JournalWatch};
use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::offering::Offering;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::seats::Seats;
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::{ReplayWith, Replayed, RestoredSession, SessionFiles};
use crate::sim_server::snapshot_dir::SnapshotDir;
use crate::sim_server::{FrameStart, SimServer, replayed};

/// What a server's match runs with from the next fixed tick, new or restored, its session built:
/// the offering its door offers, the journal the server watches and the snapshots' directory
/// when it keeps them, its seats, and the server's bots. `begin` builds it on one path for both
/// sources, and inserts it once.
#[derive(Debug)]
pub(crate) struct ServerMatch {
    offering: Offering,
    journal: Option<JournalWatch>,
    snapshots: Option<SnapshotDir>,
    seats: Seats,
    bots: ServerBots,
}

/// Where a server's match begins from.
#[derive(Debug)]
pub(crate) enum MatchSource<'a> {
    /// A new session, its lobby full.
    New(NewMatch<'a>),
    /// A session a stop ended, a crash or a failed journal, of which the server keeps the files.
    Restored(RestoredSession),
}

/// A new session's match: the log of its header, the seeds of its segments, its journal and
/// snapshots' directory if the server keeps them, and the links of its players, each with its
/// slot.
#[derive(Debug)]
pub(crate) struct NewMatch<'a> {
    pub(crate) log: SessionLog,
    pub(crate) seeds: ServerSeeds,
    pub(crate) files: Option<SessionFiles>,
    pub(crate) clients: &'a [(PlayerSlot, Entity)],
}

/// What `ServerMatch::begin` takes: the source, the packages of its mode, the server's setup,
/// the server's bots, and the session's terms as the mode's rules checked them.
#[derive(Debug)]
pub(crate) struct MatchBegin<'a> {
    pub(crate) source: MatchSource<'a>,
    pub(crate) packages: &'a ModePackages,
    pub(crate) server: &'a ServerSetup,
    pub(crate) bots: ServerBots,
    pub(crate) checked: CheckedTerms,
}

impl MatchSource<'_> {
    const fn terms(&self) -> &SessionTerms {
        match self {
            Self::New(new) => &new.log.header().terms,
            Self::Restored(restored) => restored.terms(),
        }
    }
}

impl ServerMatch {
    /// Begins the match of `begin.source` in the next fixed tick, recording into its log, each
    /// segment drawing from its seed, and runs it from then on. A frame advances the server's
    /// clock by the ticks of the max input delay less one at most, a tick at least, so a burst
    /// after a stall makes no on-time input late; a longer frame's time past that is dropped, and
    /// logged. Every unit replicates to the clients whose team sees it, and the owner's client
    /// predicts it, but a projectile or an area. With its files, the log goes into the journal
    /// as the server logs it, `JournalWatch` watches it, and the server takes the checkpoints
    /// due. The door takes the joins, and the server's bots play their slots.
    ///
    /// A new match starts at tick 0, and each of its players' links learns its slot and the
    /// start tick. A restored match builds from the snapshot of the latest checkpoint whose
    /// record the log holds, or from tick 0 with none, and replays its log to the tick the server
    /// stopped before, keeping each tick's state hash while the world holds `TickHashes`; a
    /// checkpoint begun with no record is taken again as the replay reaches its boundary, and one
    /// due at the last boundary, as a save the mode asked for, begins there. It logs
    /// `Disconnected` for each slot a player controls, whose grace period runs from then, so the
    /// ticks the stop lost take no time in the sim, and the players come back through the door.
    pub(crate) fn begin(world: &mut World, begin: MatchBegin<'_>) -> Result<(), MatchBeginError> {
        let MatchBegin {
            source,
            packages,
            server,
            bots,
            checked,
        } = begin;
        let terms = source.terms().clone();
        Self::take_terms(world, &terms, checked.visibility);
        let offering = Offering::new(terms, server);
        let signer = Arc::clone(&server.signer);
        match source {
            MatchSource::New(new) => {
                let NewMatch {
                    log,
                    seeds,
                    files,
                    clients,
                } = new;
                assert_eq!(
                    clients.len(),
                    log.header().players().count(),
                    "one client per player"
                );
                let slots = log.slot_count();
                let source = SessionSource {
                    log,
                    seeds,
                    packages,
                    checked,
                    signer: Some(signer),
                };
                Session::start(world, source).map_err(MatchBeginError::Start)?;
                let (journal, snapshots) = match files {
                    Some(SessionFiles { journal, snapshots }) => {
                        let mut session = world.resource_mut::<Session>();
                        let watch = JournalWatch::keep(&mut session, journal, JournalFrom::Start);
                        (Some(watch), Some(snapshots))
                    }
                    None => (None, None),
                };
                let run = Self {
                    offering,
                    journal,
                    snapshots,
                    seats: Seats::empty(slots),
                    bots,
                };
                run.insert(world);
                Door::seat_new(world, clients);
            }
            MatchSource::Restored(restored) => {
                let slots = restored.log.slot_count();
                let with = ReplayWith {
                    packages,
                    signer,
                    checked,
                };
                let Replayed { journal, snapshots } = restored.replay(world, with, replayed)?;
                let players: Vec<PlayerSlot> =
                    world.resource::<Session>().log().player_slots().collect();
                let now = world.resource::<Time<Real>>().elapsed();
                let run = Self {
                    offering,
                    journal: Some(journal),
                    snapshots: Some(snapshots),
                    seats: Seats::gone(slots, players.iter().copied(), now),
                    bots,
                };
                run.insert(world);
                // The stop may have come after the tick before this boundary, before the save it
                // made due here began. The session may end there, and then logs nothing more.
                SimServer::at_boundary(world);
                let mut session = world.resource_mut::<Session>();
                if session.log().result().is_none() {
                    for &slot in &players {
                        Door::serve_unless_full(&mut session, ServerInput::Disconnected { slot });
                    }
                }
            }
        }
        Ok(())
    }

    /// Takes the session's `terms` before its match begins: bounds the server's frames, so a
    /// frame advances its clock by the ticks of the max input delay less one at most, a tick at
    /// least, each as long as the clock's tick now, which a local match's pace may have set; and
    /// keeps their `visibility`, the one copy of it, which each data filter takes its audience
    /// from as the match's entities spawn.
    fn take_terms(world: &mut World, terms: &SessionTerms, visibility: Visibility) {
        let session = TickRate::new(terms.tick_hz).length();
        let tick = world.resource::<TickDuration>().0;
        assert!(
            PaceSpeed::ALL
                .iter()
                .any(|speed| speed.tick(session) == tick),
            "the server ticks at the session's rate at a speed, not every {tick:?}"
        );
        let burst = terms.inputs.max_delay.get().saturating_sub(1).max(1);
        let burst = u32::try_from(burst).expect("a max input delay of a LAN session fits u32");
        world
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(tick * burst);
        world.insert_resource(MatchVisibility(visibility));
    }

    /// Runs the match of the world's session from the next fixed tick: the door offers the
    /// offering, the server watches the journal, the checkpoints go into the snapshots'
    /// directory when the server keeps them, the players sit in the seats, and the server's bots
    /// play their slots.
    fn insert(self, world: &mut World) {
        let Self {
            offering,
            journal,
            snapshots,
            seats,
            bots,
        } = self;
        world.insert_resource(Door::new(offering));
        if let Some(journal) = journal {
            world.insert_resource(journal);
        }
        if let Some(snapshots) = snapshots {
            Checkpoints::start(world, snapshots);
        }
        let log = world.resource::<Session>().log();
        let (slots, next) = (log.slot_count(), log.next_tick());
        world.insert_resource(seats);
        world.insert_resource(Receipts::new(slots));
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start, next));
        world.insert_resource(FrameStart(next));
        let driver = BotDriver::new(bots, next, world);
        world.insert_resource(driver);
    }
}
