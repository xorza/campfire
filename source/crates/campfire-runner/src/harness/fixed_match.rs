use campfire_capabilities::Match;
use campfire_common::{PlayerSlot, Tick};
use campfire_log::ErrorReport;
use campfire_log::internals::LogCheck;
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{
    Applied, Checkpoint, InputChain, ServerInput, SessionResult, SessionTerms,
};

use crate::harness::fixed_session::FixedSession;
use crate::session::error::ServerInputRefused;
use crate::session::session_world::SessionWorld;

/// A match of a `FixedSession`, and the input chain of each slot's player, so a test sends inputs
/// as a player's client would, and server inputs as its server would.
#[derive(Debug)]
pub struct FixedMatch {
    game: Match,
    chains: Vec<InputChain>,
    terms: SessionTerms,
    applied: Vec<Applied>,
    /// Last, so it drops after the match and sees what the match logs as it drops.
    log: LogCheck,
}

impl FixedMatch {
    pub(crate) const fn new(
        game: Match,
        chains: Vec<InputChain>,
        terms: SessionTerms,
        log: LogCheck,
    ) -> FixedMatch {
        FixedMatch {
            game,
            chains,
            terms,
            applied: Vec::new(),
            log,
        }
    }

    /// Sends player `slot`'s input of `payload` stamped for `stamp`, in a packet of its own; how
    /// it applies.
    pub fn send(&mut self, slot: u32, stamp: Tick, payload: &[u8]) -> Applied {
        let chain = &mut self.chains[usize::try_from(slot).unwrap()];
        let input = chain.extend(stamp, payload);
        let signature = chain.sign(
            &Secp256k1::new(),
            &FixedSession::session_key(slot),
            self.terms.session_id(),
            &FixedSession::AUX,
        );
        self.game
            .session_mut()
            .record([input], &signature, &mut self.applied)
            .unwrap_or_else(|error| {
                panic!(
                    "player {slot}'s input at {stamp}: {}",
                    ErrorReport::of(&error)
                )
            });
        let [applied] = self.applied[..] else {
            panic!("a packet of one input applies once");
        };
        applied
    }

    /// Logs `input` before the next tick, signed by the server at the place it takes.
    pub fn serve(&mut self, input: ServerInput<'_>) -> Result<(), ServerInputRefused> {
        let place = self.game.session().log().next_place();
        let signature = FixedSession::server_signature(&input, self.terms.session_id(), place);
        self.game.session_mut().record_server(input, &signature)
    }

    /// Logs player `slot`'s join of their slot, with the delegation the session would start them
    /// with; once it is logged, the player sends on a chain started again.
    pub fn join(&mut self, slot: u32) -> Result<(), ServerInputRefused> {
        let delegation = FixedSession::delegation(&self.terms, slot);
        self.serve(ServerInput::Join {
            slot: PlayerSlot::new(slot),
            delegation,
        })?;
        self.chains[usize::try_from(slot).unwrap()] = FixedSession::chain(&self.terms, slot);
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
        let signature = FixedSession::checkpoint_signature(&record, self.terms.session_id());
        self.game
            .session_mut()
            .record_checkpoint(record.clone(), &signature)
            .expect("the match's own checkpoint");
        record
    }

    /// Ends the session before the next tick, as the mode ended the match or aborted when it did
    /// not, and publishes its log; the result, signed and logged.
    pub fn end(&mut self) -> SessionResult {
        let result = self.game.session_result();
        let signature = FixedSession::result_signature(&result, self.terms.session_id());
        self.game
            .session_mut()
            .record_result(result, &signature)
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
