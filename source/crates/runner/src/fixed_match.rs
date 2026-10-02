use campfire_math::Tick;
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{Applied, InputChain, SessionId};

use crate::fixed_session::FixedSession;
use crate::runner::Runner;

/// A match of a `FixedSession`, and each player's input chain, so a test sends inputs as a
/// player's client would.
#[derive(Debug)]
pub struct FixedMatch {
    runner: Runner,
    chains: Vec<InputChain>,
    session_id: SessionId,
    applied: Vec<Applied>,
}

impl FixedMatch {
    pub(crate) const fn new(
        runner: Runner,
        chains: Vec<InputChain>,
        session_id: SessionId,
    ) -> FixedMatch {
        FixedMatch {
            runner,
            chains,
            session_id,
            applied: Vec::new(),
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
            self.session_id,
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

    pub const fn runner(&self) -> &Runner {
        &self.runner
    }

    pub const fn runner_mut(&mut self) -> &mut Runner {
        &mut self.runner
    }

    pub fn into_runner(self) -> Runner {
        self.runner
    }
}
