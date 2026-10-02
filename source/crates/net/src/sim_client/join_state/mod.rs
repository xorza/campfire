use bevy_ecs::resource::Resource;
use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{DelegationTerms, InputChain, InputHash, SessionId, SessionTerms};
use campfire_runner::SessionRules;
use lightyear::prelude::Tick as NetTick;

use crate::error::TermsMismatch;
use crate::join::Join;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::offer::Offer;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::signer::Signer;

/// How long a delegation lets the session key sign, in seconds: a day, longer than a LAN match.
const DELEGATION_LIFETIME: u64 = 86_400;

/// Where the client is in its session; each step holds what the next one needs.
#[derive(Resource, Debug)]
pub enum JoinState {
    /// No offer came yet.
    Waiting(Joining),
    /// The offer named a session the client cannot play: it did not answer, and ended its link.
    Refused(TermsMismatch),
    /// It answered the offer, and waits for the match to start.
    Answered(JoinedSession),
    Playing(Playing),
}

/// What answers the server's offer: the player's identity, which signs the delegation, the server
/// they mean to reach, and the rules of the mode the client holds. The main key goes with the
/// answer, as nothing after it signs with that key.
#[derive(Debug)]
pub struct Joining {
    main_key: Keypair,
    server: ServerPin,
    rules: SessionRules,
    /// Unix seconds: when the delegation is made, and so when it expires.
    clock: fn() -> u64,
}

/// What the player's join fixed: the session its signatures name, what their first input links
/// to, their delegation's id, and the terms' limits on what one stamp may carry.
#[derive(Debug, Clone, Copy)]
pub struct JoinedSession {
    pub(crate) id: SessionId,
    chain_root: InputHash,
    pub(crate) max_inputs: u32,
    pub(crate) max_payload_len: u32,
}

/// The match the player plays: their session, their chain from the match start on, and the
/// clock of its ticks.
#[derive(Debug)]
pub struct Playing {
    pub(crate) session: JoinedSession,
    pub(crate) chain: InputChain,
    pub(crate) clock: MatchClock,
}

impl JoinState {
    pub(crate) const fn new(
        main_key: Keypair,
        server: ServerPin,
        rules: SessionRules,
        clock: fn() -> u64,
    ) -> JoinState {
        JoinState::Waiting(Joining {
            main_key,
            server,
            rules,
            clock,
        })
    }

    /// The clock of the match the player plays; `None` before it starts.
    pub const fn clock(&self) -> Option<MatchClock> {
        match self {
            JoinState::Playing(playing) => Some(playing.clock),
            _ => None,
        }
    }

    /// Why the client refused the offer, once it did.
    pub const fn refusal(&self) -> Option<&TermsMismatch> {
        match self {
            JoinState::Refused(mismatch) => Some(mismatch),
            _ => None,
        }
    }

    pub(crate) const fn playing_mut(&mut self) -> Option<&mut Playing> {
        match self {
            JoinState::Playing(playing) => Some(playing),
            _ => None,
        }
    }

    /// The player's answer to `offer` while the client waits for one: a delegation of the
    /// session key in the offered session, with a fresh seed contribution, and the session key's
    /// signature over the challenge and the pinned certificate hash. An error when the terms name
    /// another server, a session the client cannot play, or another rate than the listing, and
    /// the client then refuses every later offer; `None` for an offer after the first.
    pub(crate) fn answer(
        &mut self,
        offer: &Offer,
        signer: &Signer,
    ) -> Option<Result<Join, TermsMismatch>> {
        let JoinState::Waiting(joining) = self else {
            return None;
        };
        if let Err(mismatch) = joining.fits(&offer.terms) {
            *self = JoinState::Refused(mismatch.clone());
            return Some(Err(mismatch));
        }
        let now = (joining.clock)();
        let id = offer.terms.session_id();
        let granted = DelegationTerms {
            session_key: signer.public_key(),
            server_key: offer.terms.server_key,
            session_id: id,
            seed_contribution: signer.random(),
            expiration: now + DELEGATION_LIFETIME,
        };
        let delegation = signer.delegate(&joining.main_key, &granted, now);
        let answer = signer.answer(offer.challenge, &joining.server.certificate);
        *self = JoinState::Answered(JoinedSession {
            id,
            chain_root: delegation.chain_root(),
            max_inputs: offer.terms.max_inputs_per_tick,
            max_payload_len: offer.terms.max_payload_len,
        });
        Some(Ok(Join {
            delegation: delegation.json().to_owned(),
            answer,
        }))
    }

    /// Starts the match the client answered for, as `start` names it: the player's chain starts
    /// from their delegation's id. `false` when the client did not answer, or plays already.
    pub(crate) fn start(&mut self, start: MatchStart) -> bool {
        let JoinState::Answered(session) = *self else {
            return false;
        };
        *self = JoinState::Playing(Playing {
            session,
            chain: InputChain::new(start.slot, session.chain_root),
            clock: MatchClock::new(NetTick(start.start_tick)),
        });
        true
    }
}

impl Joining {
    /// Whether `terms` name the server and rate the player means to reach, and a session of the
    /// client's mode.
    fn fits(&self, terms: &SessionTerms) -> Result<(), TermsMismatch> {
        if terms.server_key != self.server.key {
            return Err(TermsMismatch::OtherServer);
        }
        self.rules.check(terms).map_err(TermsMismatch::Terms)?;
        if terms.tick_hz != self.server.tick_hz {
            return Err(TermsMismatch::OtherTickRate);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
