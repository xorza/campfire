use campfire_capabilities::Match;
use campfire_common::ErrorReport;
use campfire_common::{Seat, Tick};
use campfire_log::internals::LogCheck;
use campfire_protocol::{
    Applied, Checkpoint, Controller, InputChain, PlayerPacket, ServerInput, SessionResult,
    SessionTerms, Signer,
};

use crate::fixed_session::FixedSession;
use campfire_runner::{ServerInputRefusal, SessionWorld};

/// A match of a `FixedSession`, and the input chain of each seat's player, so a test sends inputs
/// as a player's client would, and server inputs as its server would.
#[derive(Debug)]
pub struct FixedMatch {
    game: Match,
    chains: Vec<InputChain>,
    /// Each seat's session key, which signs its chain.
    signers: Vec<Signer>,
    terms: SessionTerms,
    applied: Vec<Applied>,
    /// Last, so it drops after the match and sees what the match logs as it drops.
    log: LogCheck,
}

impl FixedMatch {
    pub(crate) const fn new(
        game: Match,
        chains: Vec<InputChain>,
        signers: Vec<Signer>,
        terms: SessionTerms,
        log: LogCheck,
    ) -> Self {
        Self {
            game,
            chains,
            signers,
            terms,
            applied: Vec::new(),
            log,
        }
    }

    /// Runs the match's next tick on the inputs it holds, as its session does.
    pub fn step(&mut self) {
        self.game.run_tick_alone();
    }

    /// Sends player `seat`'s input of `payload` stamped for `stamp`, in a packet of its own; how
    /// it applies.
    pub fn send(&mut self, seat: u32, stamp: Tick, payload: &[u8]) -> Applied {
        let at = usize::try_from(seat).unwrap();
        let chain = &mut self.chains[at];
        let input = chain.extend(stamp, payload);
        let signature = self.signers[at].sign(&chain.signed_head(self.terms.session_id()));
        self.game
            .session_mut()
            .record(
                PlayerPacket {
                    seat: input.seat,
                    inputs: [(input.stamp, input.payload)],
                    signature: &signature,
                },
                &mut self.applied,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "player {seat}'s input at {stamp}: {}",
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

    /// Logs player `seat`'s join of their seat, with the delegation the session would start them
    /// with; once it is logged, the player sends on the new chain the join starts, from the root
    /// of its place, which their server tells them.
    pub fn join(&mut self, seat: u32) -> Result<(), ServerInputRefusal> {
        let delegation = FixedSession::delegation(&self.terms, seat);
        let seat_at = Seat::new(seat);
        self.serve(ServerInput::Join {
            seat: seat_at,
            delegation,
        })?;
        let Some(Controller::Player { chain, .. }) = self.game.session().log().controller(seat_at)
        else {
            panic!("the join seats the player");
        };
        self.chains[seat_at.index()] = chain;
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

    /// The match, its keys and chains dropped, as a load leaves them behind.
    pub fn into_game(self) -> Match {
        self.game
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
