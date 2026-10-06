use campfire_common::{PlayerSlot, Tick};
use campfire_log::internals::LogCheck;
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{Applied, InputChain, ServerInput, SessionTerms};

use crate::error::ServerInputRefused;
use crate::fixed_session::FixedSession;
use crate::runner::Runner;

/// A match of a `FixedSession`, and the input chain of each slot's player, so a test sends inputs
/// as a player's client would, and server inputs as its server would.
#[derive(Debug)]
pub struct FixedMatch {
    runner: Runner,
    chains: Vec<InputChain>,
    terms: SessionTerms,
    applied: Vec<Applied>,
    /// Last, so it drops after the runner and sees what the match logs as it drops.
    log: LogCheck,
}

impl FixedMatch {
    pub(crate) const fn new(
        runner: Runner,
        chains: Vec<InputChain>,
        terms: SessionTerms,
        log: LogCheck,
    ) -> FixedMatch {
        FixedMatch {
            runner,
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
        self.runner
            .record([input], &signature, &mut self.applied)
            .unwrap_or_else(|error| panic!("player {slot}'s input at {stamp}: {error}"));
        let [applied] = self.applied[..] else {
            panic!("a packet of one input applies once");
        };
        applied
    }

    /// Logs `input` before the next tick, signed by the server at the place it takes.
    pub fn serve(&mut self, input: ServerInput) -> Result<(), ServerInputRefused> {
        let place = self.runner.log().next_place();
        let signature = FixedSession::server_signature(&input, self.terms.session_id(), place);
        self.runner.record_server(input, &signature)
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

    pub const fn runner(&self) -> &Runner {
        &self.runner
    }

    pub const fn runner_mut(&mut self) -> &mut Runner {
        &mut self.runner
    }

    /// The events at Warn and Error that the match logged and no test took yet.
    pub const fn log(&self) -> &LogCheck {
        &self.log
    }
}
