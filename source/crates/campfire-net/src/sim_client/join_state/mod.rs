use std::mem;
use std::time::Duration;

use bevy_ecs::resource::Resource;
use campfire_capabilities::Team;
use campfire_common::{PlayerSlot, Tick};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, VerifyOnly};
use campfire_protocol::{
    Delegation, DelegationId, DelegationTerms, InputChain, SeedContribution, SessionId,
    SessionTerms, SignedReceipt,
};
use campfire_runner::SessionRules;
use lightyear::prelude::{SyncedLocalTimeline, Tick as NetTick};

use crate::join::Join;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::offer::Offer;
use crate::session_times::SessionTimes;
use crate::sim_client::chain_history::ChainHistory;
use crate::sim_client::join_state::error::ReceiptRefusal;
use crate::sim_client::join_state::error::TermsMismatch;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::signer::Signer;

pub(crate) mod error;

/// How long a delegation lets the session key sign, in seconds: a day, longer than a LAN match.
const DELEGATION_LIFETIME: u64 = 86_400;
/// How long before its delegation expires a client that joins again delegates a new session key,
/// in seconds: the server checks the expiry by its own clock, which may run a little ahead.
const RENEW_MARGIN: u64 = 60;
/// The first wait before a client tries its link again, and the longest: each try doubles it.
const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(8);

/// Where the client is in its session, with the player's identity, which signs every delegation,
/// a renewal's too.
#[derive(Resource, Debug)]
pub struct JoinState {
    player: Joining,
    step: Step,
}

/// The steps of a session, each holding what the next one needs.
#[derive(Debug)]
enum Step {
    /// No offer came yet.
    Waiting,
    /// The offer named a session the client cannot play: it did not answer, and ended its link.
    Refused(TermsMismatch),
    /// It answered an offer, and waits for the match to start, or to take it up again.
    Answered(Answered),
    Playing(Playing),
    /// Its link failed: it tries the link again until the server's grace period and restore
    /// window passed.
    Rejoining(Rejoining),
    /// The player asked to leave.
    Left,
    /// It stopped, for `Loss`.
    Lost(Loss),
}

/// Why a client stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loss {
    /// Its link failed before any offer came, so it knows no grace period to wait in.
    LinkFailed,
    /// Its link stayed down past the server's grace period and restore window.
    GaveUp,
    /// The server's copy of its chain is not its own: the server rewrote the chain.
    Rewritten,
    /// A newer login of the player took the slot.
    Superseded,
}

/// What answers the server's offer: the player's identity, which signs the delegation, the server
/// they mean to reach, whether it is local, and the rules of the mode the client holds.
#[derive(Debug)]
pub(crate) struct Joining {
    main_key: Keypair,
    server: ServerPin,
    /// Whether the server runs on a thread of the client's process: only such a server loads a
    /// save, so only its match start that says so is taken as one.
    local: bool,
    rules: SessionRules,
    /// Unix seconds: when a delegation is made, and so when it expires.
    clock: fn() -> u64,
}

/// What the player's join fixed: the session its signatures name, and the terms' limits on what
/// one stamp may carry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JoinedSession {
    pub(crate) id: SessionId,
    pub(crate) max_inputs: u32,
    pub(crate) max_payload_len: u32,
}

/// What a player holds from their first answer on, through each link: the session, their
/// delegation, the id of the one it renewed, the times the offer gave, and the newest receipt the
/// server gave.
#[derive(Debug)]
pub(crate) struct Member {
    pub(crate) session: JoinedSession,
    delegation: Delegation,
    /// The delegation the current one renewed, which signed the inputs before it: a receipt of
    /// them names it.
    renewed: Option<DelegationId>,
    times: SessionTimes,
    receipt: Option<SignedReceipt>,
}

/// What a player who answered holds: what they hold as a member, and, when they played before,
/// their chain's history.
#[derive(Debug)]
struct Answered {
    member: Member,
    history: Option<ChainHistory>,
}

/// The match the player plays: what they hold as a member, their chain, its history since the
/// server's receipt or copy last met it, and the clock of the match's ticks.
#[derive(Debug)]
pub(crate) struct Playing {
    pub(crate) member: Member,
    pub(crate) chain: InputChain,
    pub(crate) clock: MatchClock,
    team: Team,
    history: ChainHistory,
}

/// A player whose link failed: what they played with, when the link failed by the client's real
/// clock, and when they try it again.
#[derive(Debug)]
struct Rejoining {
    member: Member,
    history: Option<ChainHistory>,
    lost_at: Duration,
    tries: u32,
    next_try: Duration,
}

/// What a match start did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Started {
    /// The client was not waiting for one.
    No,
    /// The player plays from the server's copy of their chain; the inputs of theirs it did not
    /// hold, `discarded`, never apply.
    Playing { discarded: u64 },
    /// The server's copy of the chain is not the player's: the client stops.
    Rewritten,
}

/// What a failed link did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkLoss {
    /// The client tries again.
    Rejoining,
    /// The client stops.
    Stopped(Loss),
    /// The client was not playing, or tries already.
    Nothing,
}

/// What a client trying its link again does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Retry {
    Wait,
    Connect,
    /// The server's grace period and restore window passed: the client stops.
    GiveUp,
}

impl JoinState {
    pub(crate) const fn new(
        main_key: Keypair,
        server: ServerPin,
        local: bool,
        rules: SessionRules,
        clock: fn() -> u64,
    ) -> JoinState {
        JoinState {
            player: Joining {
                main_key,
                server,
                local,
                rules,
                clock,
            },
            step: Step::Waiting,
        }
    }

    /// The clock of the match the player plays; `None` while it does not.
    pub const fn clock(&self) -> Option<MatchClock> {
        match &self.step {
            Step::Playing(playing) => Some(playing.clock),
            _ => None,
        }
    }

    /// The slot the player plays; `None` while they play none.
    pub const fn slot(&self) -> Option<PlayerSlot> {
        match &self.step {
            Step::Playing(playing) => Some(playing.chain.slot()),
            _ => None,
        }
    }

    /// The team the player plays in; `None` while they play none.
    pub const fn team(&self) -> Option<Team> {
        match &self.step {
            Step::Playing(playing) => Some(playing.team),
            _ => None,
        }
    }

    /// The sim tick the client runs now: its match clock's, read on `timeline` once that is synced
    /// to the server's, whose ticks the clock counts; `None` while it does not play or its
    /// timeline is not synced.
    pub fn sim_tick(&self, timeline: Option<&SyncedLocalTimeline<'_, '_>>) -> Option<Tick> {
        self.clock()?.sim_tick(timeline?.tick())
    }

    /// Why the client refused the offer, once it did.
    pub const fn refusal(&self) -> Option<&TermsMismatch> {
        match &self.step {
            Step::Refused(mismatch) => Some(mismatch),
            _ => None,
        }
    }

    /// Why the client stopped, once it did.
    pub const fn loss(&self) -> Option<Loss> {
        match self.step {
            Step::Lost(loss) => Some(loss),
            _ => None,
        }
    }

    /// Whether the player left.
    pub const fn left(&self) -> bool {
        matches!(self.step, Step::Left)
    }

    /// Whether the client tries its link again.
    pub const fn rejoining(&self) -> bool {
        matches!(self.step, Step::Rejoining(_))
    }

    pub(crate) const fn playing(&self) -> Option<&Playing> {
        match &self.step {
            Step::Playing(playing) => Some(playing),
            _ => None,
        }
    }

    pub(crate) const fn playing_mut(&mut self) -> Option<&mut Playing> {
        match &mut self.step {
            Step::Playing(playing) => Some(playing),
            _ => None,
        }
    }

    /// The player's answer to `offer`, while the client waits for one or tries to take its match
    /// up again: the delegation of the session key in the offered session, a new one with a fresh
    /// seed contribution for a first join, or for a later one whose delegation expires within
    /// `RENEW_MARGIN`, signed by a new session key; and the session key's signature over the
    /// challenge and the pinned certificate hash. An error when the terms name another server, a
    /// session the client cannot play, another rate than the listing, or, for a later join,
    /// another session, and the client then refuses every later offer; `None` for an offer it
    /// does not wait for.
    pub(crate) fn answer(
        &mut self,
        offer: &Offer,
        signer: &mut Signer,
    ) -> Option<Result<Join, TermsMismatch>> {
        let now = (self.player.clock)();
        let answered = match mem::replace(&mut self.step, Step::Left) {
            Step::Waiting => {
                if let Err(mismatch) = self.player.fits(&offer.terms) {
                    self.step = Step::Refused(mismatch.clone());
                    return Some(Err(mismatch));
                }
                Answered {
                    member: Member {
                        session: JoinedSession {
                            id: offer.terms.session_id(),
                            max_inputs: offer.terms.max_inputs_per_tick,
                            max_payload_len: offer.terms.max_payload_len,
                        },
                        delegation: self.player.delegate(&offer.terms, signer, now),
                        renewed: None,
                        times: offer.times,
                        receipt: None,
                    },
                    history: None,
                }
            }
            Step::Rejoining(rejoining) => {
                let mut member = rejoining.member;
                if offer.terms.session_id() != member.session.id {
                    let mismatch = TermsMismatch::OtherSession;
                    self.step = Step::Refused(mismatch.clone());
                    return Some(Err(mismatch));
                }
                if now.saturating_add(RENEW_MARGIN) >= member.delegation.terms().expiration {
                    signer.renew();
                    let delegation = self.player.delegate(&offer.terms, signer, now);
                    let renewed = mem::replace(&mut member.delegation, delegation);
                    member.renewed = Some(*renewed.id());
                }
                member.times = offer.times;
                Answered {
                    member,
                    history: rejoining.history,
                }
            }
            other => {
                self.step = other;
                return None;
            }
        };
        let join = Join {
            delegation: answered.member.delegation.json().to_owned(),
            answer: signer.answer(offer.challenge, &self.player.server.certificate),
        };
        self.step = Step::Answered(answered);
        Some(Ok(join))
    }

    /// Plays the match `start` names, for a client that answered: the player's chain starts from
    /// their delegation's id when the server holds none, or goes on from the server's copy, which
    /// a client with a history finds in it and cuts its own back to, and a client with none
    /// takes; after a load, which may drop inputs the client's history holds, every client of a
    /// local server takes it. Sim tick `start.first` is Lightyear tick `start.start_tick`.
    pub(crate) fn start(&mut self, start: MatchStart) -> Started {
        let answered = match mem::replace(&mut self.step, Step::Left) {
            Step::Answered(answered) => answered,
            other => {
                self.step = other;
                return Started::No;
            }
        };
        let mut member = answered.member;
        // Any other server loads no save, so its word would let it rewrite the chain unseen.
        let loaded = start.loaded && self.player.local;
        if loaded {
            // A receipt names inputs the load may have dropped.
            member.receipt = None;
        }
        let (chain, history, discarded) = match (start.chain, answered.history) {
            (None, _) => {
                let chain = InputChain::new(start.slot, member.delegation.chain_root());
                member.receipt = None;
                (chain, ChainHistory::of(&chain), 0)
            }
            // The session went back to a save: the player's chain stands where the save left it,
            // and the inputs after it never apply.
            (Some(head), Some(history)) if loaded => {
                let discarded = history.next_seq().saturating_sub(head.next_seq);
                let chain = InputChain::resume(start.slot, head.head, head.next_seq);
                (chain, ChainHistory::of(&chain), discarded)
            }
            (Some(head), Some(mut history)) => {
                if history.head_at(head.next_seq) != Some(head.head) {
                    self.step = Step::Lost(Loss::Rewritten);
                    return Started::Rewritten;
                }
                let discarded = history.cut(head.next_seq);
                let chain = InputChain::resume(start.slot, head.head, head.next_seq);
                (chain, history, discarded)
            }
            (Some(head), None) => {
                let chain = InputChain::resume(start.slot, head.head, head.next_seq);
                (chain, ChainHistory::of(&chain), 0)
            }
        };
        self.step = Step::Playing(Playing {
            member,
            chain,
            clock: MatchClock::resumed(NetTick(start.start_tick), start.first),
            team: start.team,
            history,
        });
        Started::Playing { discarded }
    }

    /// Notes that the client's link failed at `now`, by its real clock: a client that knows the
    /// server's times tries again, its chain's history kept; one that does not stops.
    pub(crate) fn lose(&mut self, now: Duration, random: [u8; 32]) -> LinkLoss {
        let (member, history) = match mem::replace(&mut self.step, Step::Left) {
            Step::Playing(playing) => (playing.member, Some(playing.history)),
            Step::Answered(answered) => (answered.member, answered.history),
            Step::Waiting => {
                self.step = Step::Lost(Loss::LinkFailed);
                return LinkLoss::Stopped(Loss::LinkFailed);
            }
            other => {
                self.step = other;
                return LinkLoss::Nothing;
            }
        };
        self.step = Step::Rejoining(Rejoining {
            member,
            history,
            lost_at: now,
            tries: 0,
            next_try: now + JoinState::wait(0, random),
        });
        LinkLoss::Rejoining
    }

    /// What a client trying its link again does at `now`, by its real clock: it connects after
    /// each wait, the next drawn with `random`, until the server's grace period and restore window
    /// passed since the link failed.
    pub(crate) fn retry(&mut self, now: Duration, random: [u8; 32]) -> Retry {
        let Step::Rejoining(rejoining) = &mut self.step else {
            return Retry::Wait;
        };
        let times = rejoining.member.times;
        let patience = times.grace + times.restore_window;
        if now.saturating_sub(rejoining.lost_at) > patience {
            self.step = Step::Lost(Loss::GaveUp);
            return Retry::GiveUp;
        }
        if now < rejoining.next_try {
            return Retry::Wait;
        }
        rejoining.tries += 1;
        rejoining.next_try = now + JoinState::wait(rejoining.tries, random);
        Retry::Connect
    }

    /// Takes `receipt`, when the server key signed it over this player's chain as it stood at
    /// its seq, under their delegation or the one it renewed, no earlier seq than the one the
    /// client keeps, as a restored server gives the same head again: the client keeps it, and
    /// forgets its chain's history before it.
    pub(crate) fn take_receipt(
        &mut self,
        receipt: &SignedReceipt,
        secp: &Secp256k1<VerifyOnly>,
    ) -> Result<(), ReceiptRefusal> {
        let Step::Playing(playing) = &mut self.step else {
            return Err(ReceiptRefusal::NotPlaying);
        };
        let signed = receipt.receipt;
        if !signed.signed_by(secp, &self.player.server.key, &receipt.signature) {
            return Err(ReceiptRefusal::BadSignature);
        }
        let member = &mut playing.member;
        if signed.session_id != member.session.id
            || signed.slot != playing.chain.slot()
            || (signed.delegation != *member.delegation.id()
                && Some(signed.delegation) != member.renewed)
        {
            return Err(ReceiptRefusal::Other);
        }
        if member
            .receipt
            .is_some_and(|kept| kept.receipt.seq > signed.seq)
        {
            return Err(ReceiptRefusal::Older);
        }
        let next_seq = signed.seq.checked_add(1).ok_or(ReceiptRefusal::OtherHead)?;
        if playing.history.head_at(next_seq) != Some(signed.head) {
            return Err(ReceiptRefusal::OtherHead);
        }
        playing.history.forget_before(next_seq);
        member.receipt = Some(*receipt);
        Ok(())
    }

    /// The newest receipt the client keeps, from its first answer until it stops.
    pub const fn receipt(&self) -> Option<&SignedReceipt> {
        let member = match &self.step {
            Step::Answered(Answered { member, .. })
            | Step::Playing(Playing { member, .. })
            | Step::Rejoining(Rejoining { member, .. }) => member,
            Step::Waiting | Step::Refused(_) | Step::Left | Step::Lost(_) => return None,
        };
        member.receipt.as_ref()
    }

    /// A newer login of the player took the slot: the client stops.
    pub(crate) fn supersede(&mut self) {
        self.step = Step::Lost(Loss::Superseded);
    }

    /// The player leaves: the client tries no link again.
    pub(crate) fn leave(&mut self) {
        self.step = Step::Left;
    }

    /// The wait before try `tries`, after the first, drawn at random below its bound, which
    /// starts at `FIRST_WAIT` and doubles to `LONGEST_WAIT`, as AWS's "full jitter" does: clients
    /// that lost one server do not all come back in one instant.
    fn wait(tries: u32, random: [u8; 32]) -> Duration {
        let bound = FIRST_WAIT
            .checked_mul(1 << tries.min(3))
            .map_or(LONGEST_WAIT, |bound| bound.min(LONGEST_WAIT));
        let draw = u64::from_le_bytes(random[..8].try_into().expect("8 bytes"));
        let nanos = (bound.as_nanos() * u128::from(draw)) >> 64;
        Duration::from_nanos(u64::try_from(nanos).expect("below the bound"))
    }
}

impl Playing {
    /// Extends the player's chain with the input of `payload` stamped `stamp`, noting its head.
    pub(crate) fn extend(&mut self, stamp: Tick, payload: &[u8]) {
        self.chain.extend(stamp, payload);
        self.history.push(self.chain.head());
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

    /// A delegation of `signer`'s session key in the session of `terms`, made at `now`, with a
    /// fresh seed contribution.
    fn delegate(&self, terms: &SessionTerms, signer: &Signer, now: u64) -> Delegation {
        let granted = DelegationTerms {
            session_key: signer.public_key(),
            server_key: terms.server_key,
            session_id: terms.session_id(),
            seed_contribution: SeedContribution::new(signer.random()),
            expiration: now + DELEGATION_LIFETIME,
        };
        signer.delegate(&self.main_key, &granted, now)
    }
}

#[cfg(test)]
mod tests;
