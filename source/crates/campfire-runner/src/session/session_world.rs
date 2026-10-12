use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{MatchEnd, MatchResult, Mode, ModeWorld, ScriptFailures};
use campfire_common::Tick;
use campfire_log::{ErrorReport, LogEvent};
use campfire_package::ModePackages;
use campfire_protocol::{
    Checkpoint, Outcome, ServerSeeds, SessionLog, SessionResult, SlotChangeKind,
    SnapshotFingerprint,
};
use campfire_sim::{
    SimRng, SimTick, SimUpdate, SimWorld, SlotEvent, SlotEventKind, TickInput, TickInputs,
};

use crate::events::script_call_failed::ScriptCallFailed;
use crate::session::Session;
use crate::session::error::{ResultMismatch, ResumeError, StartError};

/// What a session gives the world that runs its match: a bare one on a verifier, Lightyear's on
/// a server.
pub trait SessionWorld {
    /// Prepares the world, a new one, for the match of `log`'s header, of the mode `packages`
    /// holds, at the header's tick rate and with the randomness of the first segment's seed of
    /// `seeds` and the players' contributions; starts the match; and inserts the session, which
    /// records into `log` from its first tick. An error when the terms name another release,
    /// mode or dependencies than this release and `packages`, a tick rate outside the mode's
    /// range, or more slots than the mode's teams have, when `seeds` are not the seeds of the
    /// chain the header commits to or do not reach the last segment of `log`, when a change of
    /// a slot's controller the log holds, as a published log replayed does, breaks the mode's
    /// `[players]`, or when the packages do not load into the match.
    fn start_session(
        &mut self,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<(), StartError>;

    /// Prepares the world, a new one, for the match of `log`, rewound to replay its ticks, from
    /// the checkpoint of it that starts `segment`, as a restore does: builds the match as
    /// `start_session` does, but starts no mode, as the state is the checkpoint's `snapshot`;
    /// and seals the log's ticks before the checkpoint without running them, so the next tick
    /// the session runs is the checkpoint's. An error as `start_session` gives one, and when the
    /// log holds no such checkpoint, when `snapshot` is not the one the checkpoint
    /// fingerprints, when it does not restore, or restores to another state hash than the
    /// checkpoint's.
    fn resume_session(
        &mut self,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
        snapshot: &[u8],
    ) -> Result<(), ResumeError>;

    /// The session of the match.
    fn session(&self) -> &Session;

    /// The session of the match, to change.
    fn session_mut(&mut self) -> Mut<'_, Session>;

    /// Seals the next tick in the session's log and runs it with the inputs and the changes of
    /// a slot's controller applied in it, drawing from the seed of the segment that starts there
    /// when one does, and logs each script call of the tick that failed.
    fn run_session_tick(&mut self);

    /// Runs the next tick as `run_session_tick` does, in a world no `App` updates, a replay, a
    /// verification or a restore before its server's first frame, then clears its change
    /// trackers, as an `App`'s update does each frame: its removed-component messages keep the
    /// tick's removals alone, so a long replay holds them in bounded memory. The sim reads none
    /// of them, and tracks its own changes, so its state is the same either way.
    fn run_tick_alone(&mut self);

    /// Whether a save is due at the boundary before the next tick: the mode asked for one in
    /// the tick before, by `ctx.save()`, or the boundary is at a multiple of the mode's
    /// `[saves] autosave_ms` from the session's start.
    fn save_due(&self) -> bool;

    /// The record of the checkpoint the log began and has no record of, of the state at its
    /// boundary, its snapshot written into `snapshot`, which is cleared first; none when the log
    /// began none.
    fn checkpoint(&self, snapshot: &mut Vec<u8>) -> Option<Checkpoint>;

    /// How the match ended: as the mode ended it, or aborted when it did not.
    fn outcome(&self) -> Outcome;

    /// The result that ends the session before the next tick as `outcome`, with the state hash.
    fn session_result_as(&self, outcome: Outcome) -> SessionResult;

    /// The result that ends the session before the next tick, as the mode ended the match or
    /// aborted when it did not.
    fn session_result(&self) -> SessionResult;

    /// Whether the log's result, once every tick before it ran, holds: its state hash is the
    /// state's, and its outcome the one the mode ended the match with, unless the session
    /// aborted.
    fn check_result(&self) -> Result<(), ResultMismatch>;
}

impl SessionWorld for World {
    fn start_session(
        &mut self,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<(), StartError> {
        assert_eq!(
            log.next_tick(),
            Tick::new(0),
            "a session starts before its first tick"
        );
        let session = Session::build(self, log, seeds, packages, 0)?;
        Mode::start(self).map_err(StartError::MatchStart)?;
        self.insert_resource(session);
        Ok(())
    }

    fn resume_session(
        &mut self,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
        snapshot: &[u8],
    ) -> Result<(), ResumeError> {
        assert_eq!(
            log.next_tick(),
            Tick::new(0),
            "a resumed log replays from its first tick"
        );
        let record = log
            .checkpoints()
            .find(|record| record.segment == segment)
            .ok_or(ResumeError::NoCheckpoint)?;
        let (tick, state_hash) = (record.tick, record.state_hash);
        if SnapshotFingerprint::of(snapshot) != record.snapshot {
            return Err(ResumeError::Fingerprint);
        }
        let mut session =
            Session::build(self, log, seeds, packages, segment).map_err(ResumeError::Start)?;
        self.restore(snapshot).map_err(ResumeError::Snapshot)?;
        if self.state_hash() != state_hash {
            return Err(ResumeError::StateHash);
        }
        session.log.seal_until(tick);
        self.insert_resource(session);
        Ok(())
    }

    fn session(&self) -> &Session {
        self.resource::<Session>()
    }

    fn session_mut(&mut self) -> Mut<'_, Session> {
        self.resource_mut::<Session>()
    }

    fn run_session_tick(&mut self) {
        self.resource_scope(|world, mut session: Mut<'_, Session>| {
            debug_assert_eq!(
                world.resource::<SimTick>().start(),
                session.log.next_tick(),
                "the sim and the log are at the same tick"
            );
            let next = session.log.next_tick();
            if let Some(segment) = session
                .log
                .segment_starting(next)
                .filter(|&segment| segment > session.segment)
            {
                let server_seed = session
                    .seeds
                    .seed(segment)
                    .expect("the seeds reach the log's last segment");
                let seed = session
                    .log
                    .header()
                    .segment_seed(segment, &server_seed)
                    .expect("seeds whose first checks give every segment's");
                world.insert_resource(SimRng::new(seed));
                session.segment = segment;
            }
            let mut inputs = world.resource_mut::<TickInputs>();
            for input in session.log.seal_tick() {
                inputs.push(TickInput {
                    slot: input.slot,
                    payload: input.payload,
                });
            }
            for change in session.log.sealed_changes() {
                let kind = match change.kind {
                    SlotChangeKind::Joined { .. } => SlotEventKind::Joined,
                    SlotChangeKind::Left { .. } => SlotEventKind::Left,
                };
                inputs.push_slot_event(SlotEvent {
                    slot: change.slot,
                    kind,
                });
            }
        });
        let tick = self.resource::<SimTick>().start();
        self.run_schedule(SimUpdate);
        if let Some(failures) = self.get_non_send::<ScriptFailures>() {
            for failure in failures.get() {
                ScriptCallFailed {
                    tick,
                    unit: failure.id,
                    hook: failure.hook,
                    error: ErrorReport::of(&failure.error).to_string(),
                }
                .log();
            }
        }
    }

    fn run_tick_alone(&mut self) {
        self.run_session_tick();
        self.clear_trackers();
    }

    fn save_due(&self) -> bool {
        let session = self.session();
        let next = session.log.next_tick();
        let asked = self.save_asked() == Some(next);
        let autosave = session
            .autosave
            .is_some_and(|every| next.get() > 0 && next.get().is_multiple_of(every.get()));
        asked || autosave
    }

    fn checkpoint(&self, snapshot: &mut Vec<u8>) -> Option<Checkpoint> {
        let begun = self.session().log.begun_checkpoint()?;
        debug_assert_eq!(
            self.resource::<SimTick>().start(),
            begun.tick,
            "the state stands at the checkpoint's boundary"
        );
        let state_hash = self.snapshot(snapshot);
        Some(Checkpoint {
            segment: begun.segment,
            tick: begun.tick,
            state_hash,
            snapshot: SnapshotFingerprint::of(snapshot),
            carry: begun.carry.clone(),
        })
    }

    fn outcome(&self) -> Outcome {
        match self.match_end().map(MatchEnd::result) {
            Some(MatchResult::Won(team)) => Outcome::Won { team: team.get() },
            Some(MatchResult::Draw) => Outcome::Draw,
            None => Outcome::Aborted,
        }
    }

    fn session_result_as(&self, outcome: Outcome) -> SessionResult {
        SessionResult {
            tick: self.session().log.next_tick(),
            outcome,
            state_hash: self.state_hash(),
        }
    }

    fn session_result(&self) -> SessionResult {
        self.session_result_as(self.outcome())
    }

    fn check_result(&self) -> Result<(), ResultMismatch> {
        let log = &self.session().log;
        let Some(result) = log.result() else {
            return Ok(());
        };
        assert_eq!(
            result.tick,
            log.next_tick(),
            "a result is checked once every tick before it ran"
        );
        let replayed = self.state_hash();
        if result.state_hash != replayed {
            return Err(ResultMismatch::Hash {
                logged: result.state_hash,
                replayed,
            });
        }
        let ended = self.outcome();
        if result.outcome != Outcome::Aborted && result.outcome != ended {
            return Err(ResultMismatch::Outcome {
                logged: result.outcome,
                ended,
            });
        }
        Ok(())
    }
}
