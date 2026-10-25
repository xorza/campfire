use std::thread;

use campfire_capabilities::Match;
use campfire_common::ErrorReport;
use campfire_common::{PlayerSlot, Tick};
use campfire_log::internals::LogCheck;
use campfire_protocol::{
    Applied, Checkpoint, Controller, Delegation, InputChain, PlayerPacket, ServerInput,
    SessionResult, SessionTerms, Signer,
};

use crate::fixed_session::FixedSession;
use crate::probe_file::ProbeFile;
use campfire_runner::{BotInputDrop, ServerInputRefusal, SessionWorld};
use campfire_sim::{UnreadCommand, UnreadCommands};

/// A match of a `FixedSession`, and the input chain of each player slot's player, so a test sends
/// inputs as a player's client would, and server inputs as its server would.
#[derive(Debug)]
pub struct FixedMatch {
    game: Match,
    chains: Vec<InputChain>,
    /// Each player slot's session key, which signs its chain.
    signers: Vec<Signer>,
    terms: SessionTerms,
    applied: Vec<Applied>,
    probe: PendingProbe,
    /// Last, so it drops after the match and sees what the match logs as it drops.
    log: LogCheck,
}

/// The probe of a fixed match, which writes its file as it drops, a failing test's among them, so
/// a test shows its units by `CAMPFIRE_PROBE` with no edit (02-engine-core.md, One probe of a
/// match).
#[derive(Debug)]
struct PendingProbe(Option<ProbeFile>);

/// A match and the check of what it logs, the check last, so it drops after the match and sees
/// what the match logs as it drops.
#[derive(Debug)]
pub struct CheckedMatch {
    pub game: Match,
    pub log: LogCheck,
}

impl FixedMatch {
    pub(crate) fn new(
        game: Match,
        chains: Vec<InputChain>,
        signers: Vec<Signer>,
        terms: SessionTerms,
        log: LogCheck,
    ) -> Self {
        let probe = ProbeFile::of_env(&game, terms.session_id());
        Self {
            game,
            chains,
            signers,
            terms,
            applied: Vec::new(),
            probe: PendingProbe(probe),
            log,
        }
    }

    /// Runs the match's next tick on the inputs it holds, as its session does, and records it in
    /// its probe, if any.
    pub fn step(&mut self) {
        self.run_tick();
    }

    /// Runs the match's next tick as `step` does, and takes the commands nothing read, which a
    /// play that counts them, as the Zero Hour check's, reads.
    pub fn step_unread(&mut self) -> Vec<UnreadCommand> {
        self.run_tick();
        UnreadCommands::take(&mut self.game)
    }

    /// Runs the match's next tick on the inputs it holds, and records it in its probe, if any.
    fn run_tick(&mut self) {
        self.game.run_tick_alone();
        if let Some(probe) = &mut self.probe.0 {
            probe.probe.record(&self.game);
        }
    }

    /// Probes the match by `probe` from its next tick, in place of the probe `CAMPFIRE_PROBE`
    /// asked, whose file is written then.
    pub fn probe(&mut self, probe: ProbeFile) {
        self.probe = PendingProbe(Some(probe));
    }

    /// Takes the match's probe, which it then writes no more, for a caller that writes it.
    pub const fn take_probe(&mut self) -> Option<ProbeFile> {
        self.probe.0.take()
    }

    /// Sends player `player_slot`'s input of `payload` stamped for `stamp`, in a packet of its own;
    /// how it applies.
    pub fn send(&mut self, player_slot: u32, stamp: Tick, payload: &[u8]) -> Applied {
        let at = usize::try_from(player_slot).unwrap();
        let chain = &mut self.chains[at];
        let input = chain.extend(stamp, payload);
        let signature = self.signers[at].sign(&chain.signed_head(self.terms.session_id()));
        self.game
            .session_mut()
            .record(
                PlayerPacket {
                    player_slot: input.player_slot,
                    inputs: [(input.stamp, input.payload)],
                    signature: &signature,
                },
                &mut self.applied,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "player {player_slot}'s input at {stamp}: {}",
                    ErrorReport::of(&error)
                )
            });
        let [applied] = self.applied[..] else {
            panic!("a packet of one input applies once");
        };
        applied
    }

    /// Logs `input` before the next tick, signed by the server at the place it takes.
    pub fn serve(&mut self, input: ServerInput<'_>) -> Result<(), ServerInputRefusal> {
        self.game.session_mut().serve(input)
    }

    /// Logs `payload` as an input of the bot of `player_slot`, as the server's bot driver logs it;
    /// the cause the log refused it for, which drops it.
    pub fn serve_bot(
        &mut self,
        player_slot: PlayerSlot,
        payload: &[u8],
    ) -> Result<(), BotInputDrop> {
        self.game.session_mut().serve_bot(player_slot, payload)
    }

    /// The delegation the session starts player `player` with, of `TestKey::player(player)`.
    pub fn delegation(&self, player: u32) -> Delegation {
        FixedSession::delegation(&self.terms, player)
    }

    /// Logs player `player_slot`'s join of their player slot, with the delegation the session would
    /// start them with; once it is logged, the player sends on the new chain the join starts, from
    /// the root of its place, which their server tells them.
    pub fn join(&mut self, player_slot: u32) -> Result<(), ServerInputRefusal> {
        let delegation = self.delegation(player_slot);
        let player_slot_at = PlayerSlot::new(player_slot);
        self.serve(ServerInput::Join {
            player_slot: player_slot_at,
            delegation,
        })?;
        let Some(Controller::Player { chain, .. }) =
            self.game.session().log().controller(player_slot_at)
        else {
            panic!("the join places the player");
        };
        self.chains[player_slot_at.index()] = chain;
        Ok(())
    }

    /// Checkpoints the match at the boundary before the next tick, as its server would, its
    /// snapshot written into `snapshot`; the record, signed and logged.
    pub fn checkpoint(&mut self, snapshot: &mut Vec<u8>) -> Checkpoint {
        self.game
            .session_mut()
            .begin_checkpoint()
            .expect("the match's own checkpoint");
        let record = self
            .game
            .checkpoint(snapshot)
            .expect("a fixed session's chain holds the segment");
        self.game
            .session_mut()
            .record_checkpoint(record.clone())
            .expect("the match's own checkpoint");
        record
    }

    /// Ends the session before the next tick, as the mode ended the match or aborted when it did
    /// not, and publishes its log; the result, signed and logged.
    pub fn end(&mut self) -> SessionResult {
        let result = self.game.session_result();
        self.game
            .session_mut()
            .record_result(result)
            .expect("the match's own result");
        self.game.session_mut().reveal_seed();
        result
    }

    /// The match, its keys and chains dropped, as a load leaves them behind, with the check of
    /// what it logs, so a warning it logs after still fails the test.
    pub fn into_checked(self) -> CheckedMatch {
        CheckedMatch {
            game: self.game,
            log: self.log,
        }
    }

    /// The tick it runs next: how many ticks its session ran.
    pub fn next_tick(&self) -> Tick {
        self.game.session().log().next_tick()
    }

    pub const fn game(&self) -> &Match {
        &self.game
    }

    pub const fn game_mut(&mut self) -> &mut Match {
        &mut self.game
    }

    /// The events at Warn and Error that the match logged and no test took yet.
    pub const fn log(&self) -> &LogCheck {
        &self.log
    }
}

impl Drop for PendingProbe {
    /// Writes the probe's file; a file that does not write fails the test, unless it fails
    /// already, as a panic in a panic aborts the test with no message.
    fn drop(&mut self) {
        let Some(probe) = self.0.take() else {
            return;
        };
        if let Err(error) = probe.write()
            && !thread::panicking()
        {
            panic!("{}", ErrorReport::of(&error));
        }
    }
}
