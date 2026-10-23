use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_time::{Time, Virtual};
use campfire_common::PlayerSlot;
use campfire_log::LogEvent;
use campfire_net_timeline::{Clock, LocalTimeline, TimelineRate};
use campfire_package::ModePackages;
use campfire_protocol::{ServerInput, ServerSeeds, SessionLog, SessionTerms};
use campfire_runner::{CheckedTerms, Session, SessionSource};
use campfire_sim::MatchRate;

use crate::sim_server::door::Door;
use crate::sim_server::offering::Offering;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::{FrameStart, SimServerPlugin, replayed};
use campfire_host::BotDriver;
use campfire_host::Checkpoints;
use campfire_host::MatchBeginError;
use campfire_host::MatchClock;
use campfire_host::PaceSpeed;
use campfire_host::PlayerSlotLinks;
use campfire_host::SaveLoaded;
use campfire_host::ServerBots;
use campfire_host::ServerSetup;
use campfire_host::SnapshotDir;
use campfire_host::{JournalFrom, JournalWatch};
use campfire_host::{ReplayWith, Replayed, RestoredSession, SessionFiles};

/// What a server's match runs with from the next fixed tick, new or restored, its session built:
/// the offering its door offers, the journal the server watches and the snapshots' directory when
/// it keeps them, its player slots, and the server's bots. `begin` builds it on one path for both
/// sources, and inserts it once.
#[derive(Debug)]
pub(crate) struct ServerMatch {
    offering: Offering,
    journal: Option<JournalWatch>,
    snapshots: Option<SnapshotDir>,
    player_slot_links: PlayerSlotLinks,
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
/// player slot.
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

/// The most ticks a server frame runs after a stall, the time past them dropped and logged, so a
/// frame costs at most this many ticks' work and a stall never feeds a longer frame after it, as
/// a fixed time step bounds its catch-up (08-sessions.md, A burst after a stall runs 9 ticks at
/// most).
const MAX_FRAME_TICKS: u32 = 9;

impl ServerMatch {
    /// Begins the match of `begin.source` in the next fixed tick, recording into its log, each
    /// segment drawing from its seed, and runs it from then on. A frame advances the server's
    /// clock by `MAX_FRAME_TICKS` at most; a longer frame's time past that is dropped, and
    /// logged. Every unit replicates to the clients whose team sees it, and the owner's client
    /// predicts it, but a projectile or an area. With its files, the log goes into the journal
    /// as the server logs it, `JournalWatch` watches it, and the server takes the checkpoints
    /// due. The door takes the joins, and the server's bots play their player slots.
    ///
    /// A new match starts at tick 0, and each of its players' links learns its player slot and the
    /// start tick. A restored match builds from the snapshot of the latest checkpoint whose record
    /// the log holds, or from tick 0 with none, and replays its log to the tick the server stopped
    /// before, keeping each tick's state hash while the world holds `TickHashes`; a checkpoint
    /// begun with no record is taken again as the replay reaches its boundary, and one due at the
    /// last boundary, as a save the mode asked for, begins there. It logs `Disconnected` for each
    /// player slot a player controls, whose grace period runs from then, so the ticks the stop lost
    /// take no time in the sim, and the players come back through the door.
    pub(crate) fn begin(world: &mut World, begin: MatchBegin<'_>) -> Result<(), MatchBeginError> {
        let MatchBegin {
            source,
            packages,
            server,
            bots,
            checked,
        } = begin;
        let terms = source.terms().clone();
        Self::take_terms(world, &terms);
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
                let player_slots = log.player_slot_count();
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
                    player_slot_links: PlayerSlotLinks::empty(player_slots),
                    bots,
                };
                run.insert(world);
                Door::player_slot_new(world, clients);
            }
            MatchSource::Restored(restored) => {
                let player_slots = restored.log.player_slot_count();
                let with = ReplayWith {
                    packages,
                    signer,
                    checked,
                };
                let Replayed { journal, snapshots } = restored.replay(world, with, replayed)?;
                let players: Vec<PlayerSlot> = world
                    .resource::<Session>()
                    .log()
                    .played_player_slots()
                    .collect();
                let now = world.resource::<Clock>().now().elapsed();
                let run = Self {
                    offering,
                    journal: Some(journal),
                    snapshots: Some(snapshots),
                    player_slot_links: PlayerSlotLinks::gone(
                        player_slots,
                        players.iter().copied(),
                        now,
                    ),
                    bots,
                };
                run.insert(world);
                // The stop may have come after the tick before this boundary, before the save it
                // made due here began. The session may end there, and then logs nothing more.
                SimServerPlugin::at_boundary(world);
                let mut session = world.resource_mut::<Session>();
                if session.log().result().is_none() {
                    for &player_slot in &players {
                        Door::serve_unless_full(
                            &mut session,
                            ServerInput::Disconnected { player_slot },
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Takes the session's `terms` before its match begins: bounds the server's frames, so a
    /// frame advances its clock by `MAX_FRAME_TICKS` at most, each as long as the clock's tick
    /// now, which a local match's pace may have set. The
    /// match's `MatchFacts` keep their visibility, the one copy of it, which each data filter
    /// takes its audience from as the match's entities spawn.
    fn take_terms(world: &mut World, terms: &SessionTerms) {
        let session = MatchRate::new(terms.tick_hz).rate();
        let rate = world.resource::<TimelineRate>().0;
        assert!(
            PaceSpeed::ALL
                .iter()
                .any(|speed| speed.rate(session) == rate),
            "the server ticks at the session's rate at a speed, not at {rate:?}"
        );
        let tick = rate.period();
        world
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(tick * MAX_FRAME_TICKS);
    }

    /// Runs the match on from the save its session went back to in place (08-sessions.md, A load
    /// goes back in place): the match's clock and the frame start from the save's tick, the bots
    /// play again from it, no player holds a receipt of the inputs the load dropped, and every
    /// joined link takes the next offer, as `Door::offer_again` says. Each player slot a player
    /// controls is gone from then, as after a restore, and logs `Disconnected`, until its player
    /// joins again.
    pub(crate) fn follow_load(world: &mut World) {
        let log = world.resource::<Session>().log();
        let (id, player_slots, next) = (log.session_id(), log.player_slot_count(), log.next_tick());
        let players: Vec<PlayerSlot> = log.played_player_slots().collect();
        world.insert_resource(Receipts::new(player_slots));
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start, next));
        world.insert_resource(FrameStart(next));
        BotDriver::load(world, next);
        let now = world.resource::<Clock>().now().elapsed();
        let links = PlayerSlotLinks::gone(player_slots, players.iter().copied(), now);
        world.insert_resource(links);
        Door::offer_again(world, next);
        let mut session = world.resource_mut::<Session>();
        for &player_slot in &players {
            Door::serve_unless_full(&mut session, ServerInput::Disconnected { player_slot });
        }
        SaveLoaded {
            session: id,
            tick: next,
        }
        .log();
    }

    /// Runs the match of the world's session from the next fixed tick: the door offers the
    /// offering, the server watches the journal, the checkpoints go into the snapshots' directory
    /// when the server keeps them, the players sit in the player slots, and the server's bots play
    /// their player slots.
    fn insert(self, world: &mut World) {
        let Self {
            offering,
            journal,
            snapshots,
            player_slot_links,
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
        let (player_slots, next) = (log.player_slot_count(), log.next_tick());
        world.insert_resource(player_slot_links);
        world.insert_resource(Receipts::new(player_slots));
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start, next));
        world.insert_resource(FrameStart(next));
        let driver = BotDriver::new(bots, next, world);
        world.insert_resource(driver);
    }
}
