use std::mem;
use std::sync::Arc;
use std::time::Duration;

use bevy_ecs::resource::Resource;
use campfire_common::{ChannelBinding, PlayerSlot, Tick};
use campfire_net_core::NetTick;
use campfire_package::ModePackages;
use campfire_protocol::{
    ConnectAnswer, Delegation, DelegationId, DelegationTerms, InputChain, InputRules,
    SeedContribution, SessionId, SessionTerms, SignedReceipt, Signer, Verifier,
};
use campfire_runner::SessionRules;
use campfire_values::{Team, TeamSet};

use crate::client::chain_history::ChainHistory;
use crate::client::join_state::error::TermsMismatch;
use crate::client::server_pin::ServerPin;
use crate::error::ReceiptRefusal;
use crate::hosting::Hosting;
use crate::match_clock::MatchClock;
use crate::messages::dismissed::Dismissed;
use crate::messages::join::{Join, JoinRole};
use crate::messages::match_start::{MatchStart, PlayerSlotChain};
use crate::messages::offer::SessionOffer;
use crate::messages::watch_start::WatchStart;
use crate::session_times::SessionTimes;

pub(crate) mod error;

/// How long a delegation lets the session key sign, in seconds: a day, longer than a LAN match.
const DELEGATION_LIFETIME: u64 = 86_400;
/// How long before its delegation expires a client that joins again delegates a new session key,
/// in seconds: the server checks the expiry by its own clock, which may run a little ahead.
const RENEW_MARGIN: u64 = 60;
/// The first wait before a client tries its link again, and the longest: each try doubles it.
const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(8);
/// How many times the wait doubles from `FIRST_WAIT` to `LONGEST_WAIT`.
const DOUBLINGS: u32 = {
    let ratio = LONGEST_WAIT.as_nanos() / FIRST_WAIT.as_nanos();
    assert!(
        ratio.is_power_of_two() && FIRST_WAIT.as_nanos() * ratio == LONGEST_WAIT.as_nanos(),
        "the longest wait is the first doubled"
    );
    ratio.ilog2()
};

/// Where the client is in its session, with the player's identity, which signs every delegation,
/// a renewal's too, and a spectator's choice of teams.
#[derive(Resource, Debug)]
pub struct JoinState {
    player: Joining,
    step: Step,
    /// Whether the server holds the spectator's choice of teams: its join carried it, or the
    /// client told it since.
    teams_told: bool,
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
    /// It watches the match, with no player slot.
    Watching(Watching),
    /// Its link failed: it tries the link again until the server's grace period and restore
    /// window passed.
    Rejoining(Rejoining),
    /// The session went back to a save, its link kept: it waits for the server's next offer.
    Loaded(Answered),
    /// The player asked to leave while they played, and their link is still open: a receipt of
    /// the inputs they sent may still come.
    Leaving(Leaving),
    /// The player left, and their link closed, or they left before they played.
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
    /// A newer login of the player took the player slot.
    Superseded,
    /// The server refused the player's join; its log says why.
    JoinRefused,
}

/// What answers the server's offer: the player's identity, which signs the delegation, the server
/// they mean to reach, whether they play or watch, and the packages of the mode the client holds.
#[derive(Debug)]
pub struct Joining {
    pub main: Arc<Signer>,
    pub server: ServerPin,
    pub role: JoinRole,
    pub packages: Arc<ModePackages>,
}

/// What the player's join fixed: the session its signatures name, and the terms' limits on its
/// inputs.
#[derive(Debug, Clone, Copy)]
pub struct JoinedSession {
    pub id: SessionId,
    pub inputs: InputRules,
}

/// What a player holds from their first answer on, through each link: the session, their
/// delegation, the id of the one it renewed, the times the offer gave, and the newest receipt the
/// server gave.
#[derive(Debug)]
pub struct Member {
    pub session: JoinedSession,
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
pub struct Playing {
    pub member: Member,
    pub chain: InputChain,
    pub clock: MatchClock,
    team: Team,
    history: ChainHistory,
    /// The last stamp the log holds of the player, below which it refuses the next.
    last_stamp: Option<Tick>,
}

/// A spectator that watches the match: what it holds as a member, and the clock of the match's
/// ticks.
#[derive(Debug)]
struct Watching {
    member: Member,
    clock: MatchClock,
}

/// A player who left while they played: what they hold as a member, their player slot and their
/// chain's history, which a receipt the server sent before it took the leave is checked against.
#[derive(Debug)]
struct Leaving {
    member: Member,
    player_slot: PlayerSlot,
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
pub enum StartEffect {
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
pub enum LinkLoss {
    /// The client tries again.
    Rejoining,
    /// The client stops.
    Stopped(Loss),
    /// The client was not playing, or tries already.
    Nothing,
}

/// What a client trying its link again does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    Wait,
    Connect,
    /// The server's grace period and restore window passed: the client stops.
    GiveUp,
}

impl JoinState {
    pub const fn new(player: Joining) -> Self {
        Self {
            player,
            step: Step::Waiting,
            teams_told: false,
        }
    }

    /// The clock of the match the player plays; `None` while it does not.
    pub const fn playing_clock(&self) -> Option<MatchClock> {
        match &self.step {
            Step::Playing(playing) => Some(playing.clock),
            _ => None,
        }
    }

    /// The clock of the match the client watches as a spectator; `None` while it does not.
    pub const fn watching_clock(&self) -> Option<MatchClock> {
        match &self.step {
            Step::Watching(watching) => Some(watching.clock),
            _ => None,
        }
    }

    /// The teams whose views the client watches together, in a session whose spectators watch
    /// teams' views; `None` while it does not watch.
    pub const fn watched_teams(&self) -> Option<TeamSet> {
        match (&self.step, self.player.role) {
            (Step::Watching(_), JoinRole::Watch { teams }) => Some(teams),
            _ => None,
        }
    }

    /// Chooses the teams whose views the spectator watches together, which the server takes
    /// once the client watches, and each join after carries.
    pub fn choose_teams(&mut self, teams: TeamSet) {
        let JoinRole::Watch { teams: chosen } = &mut self.player.role else {
            panic!("a client that plays chooses no teams");
        };
        if *chosen != teams {
            *chosen = teams;
            self.teams_told = false;
        }
    }

    /// The spectator's choice of teams to tell the server, once, while the client watches and the
    /// server does not hold it.
    pub fn untold_teams(&mut self) -> Option<TeamSet> {
        let teams = self.watched_teams().filter(|_| !self.teams_told)?;
        self.teams_told = true;
        Some(teams)
    }

    /// The player slot the player plays; `None` while they play none.
    pub const fn player_slot(&self) -> Option<PlayerSlot> {
        match &self.step {
            Step::Playing(playing) => Some(playing.chain.player_slot()),
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

    /// The sim tick the client runs now: its match clock's at `now`, the tick of its timeline once
    /// that is synced to the server's, whose ticks the clock counts; `None` while it does not play
    /// or its timeline is not synced.
    pub fn sim_tick(&self, now: Option<NetTick>) -> Option<Tick> {
        self.playing_clock()?.sim_tick(now?)
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
    pub const fn is_left(&self) -> bool {
        matches!(self.step, Step::Leaving(_) | Step::Left)
    }

    /// Whether the client tries its link again.
    pub const fn is_rejoining(&self) -> bool {
        matches!(self.step, Step::Rejoining(_))
    }

    pub const fn playing(&self) -> Option<&Playing> {
        match &self.step {
            Step::Playing(playing) => Some(playing),
            _ => None,
        }
    }

    pub const fn playing_mut(&mut self) -> Option<&mut Playing> {
        match &mut self.step {
            Step::Playing(playing) => Some(playing),
            _ => None,
        }
    }

    /// The player's answer to `offer`, at `now` in Unix seconds, while the client waits for one or
    /// tries to take its match up again: the delegation of the session key in the offered session,
    /// made at `now`, a new one with a fresh seed contribution for a first join, or for a later one
    /// whose delegation expires within `RENEW_MARGIN`, signed by a new session key; and the session
    /// key's signature over the challenge and `binding`, the link's TLS session's. An error when
    /// the terms name another server, a session the client cannot play, another rate than the
    /// listing, or, for a later join, another session, and the client then refuses every later
    /// offer; `None` for an offer it does not wait for.
    pub fn answer(
        &mut self,
        offer: &SessionOffer,
        binding: &ChannelBinding,
        signer: &mut Signer,
        now: u64,
    ) -> Option<Result<Join, TermsMismatch>> {
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
                            inputs: offer.terms.inputs,
                        },
                        delegation: self.player.delegate(&offer.terms, signer, now),
                        renewed: None,
                        times: offer.times,
                        receipt: None,
                    },
                    history: None,
                }
            }
            Step::Rejoining(Rejoining {
                member, history, ..
            })
            | Step::Loaded(Answered { member, history }) => {
                let mut member = member;
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
                Answered { member, history }
            }
            other => {
                self.step = other;
                return None;
            }
        };
        let join = Join {
            delegation: answered.member.delegation.json().to_owned(),
            answer: signer.sign(&ConnectAnswer {
                challenge: &offer.challenge,
                binding,
            }),
            role: self.player.role,
        };
        self.teams_told = true;
        self.step = Step::Answered(answered);
        Some(Ok(join))
    }

    /// Plays the match `start` names, for a client that answered: the player's chain starts anew
    /// from the root the server names, whatever the client's history, or goes on from the server's
    /// copy, which a client with a history finds in it and cuts its own back to, and a client with
    /// none takes; after a load, which may drop inputs the client's history holds, every client of
    /// a local server takes it: only a local server loads a save, so only the match start of a
    /// server `hosting` says is local is taken as a load. The client runs by `start.clock`.
    pub fn start(&mut self, start: MatchStart, hosting: Hosting) -> StartEffect {
        let answered = match mem::replace(&mut self.step, Step::Left) {
            Step::Answered(answered) if self.player.role == JoinRole::Play => answered,
            other => {
                self.step = other;
                return StartEffect::No;
            }
        };
        let mut member = answered.member;
        // Any other server loads no save, so its word would let it rewrite the chain unseen.
        let loaded = start.loaded && hosting == Hosting::Local;
        if loaded {
            // A receipt names inputs the load may have dropped.
            member.receipt = None;
        }
        let (chain, history, discarded, last_stamp) = match (start.chain, answered.history) {
            (PlayerSlotChain::New { root }, _) => {
                let chain = InputChain::new(start.player_slot, root);
                member.receipt = None;
                (chain, ChainHistory::of(&chain), 0, None)
            }
            // The session went back to a save: the player's chain stands where the save left it,
            // and the inputs after it never apply.
            (PlayerSlotChain::Held(head), Some(history)) if loaded => {
                let discarded = history.next_seq().saturating_sub(head.next_seq);
                let chain = InputChain::resume(start.player_slot, head.head, head.next_seq);
                (chain, ChainHistory::of(&chain), discarded, head.last_stamp)
            }
            (PlayerSlotChain::Held(head), Some(mut history)) => {
                if history.head_at(head.next_seq) != Some(head.head) {
                    self.step = Step::Lost(Loss::Rewritten);
                    return StartEffect::Rewritten;
                }
                let discarded = history.cut(head.next_seq);
                let chain = InputChain::resume(start.player_slot, head.head, head.next_seq);
                (chain, history, discarded, head.last_stamp)
            }
            (PlayerSlotChain::Held(head), None) => {
                let chain = InputChain::resume(start.player_slot, head.head, head.next_seq);
                (chain, ChainHistory::of(&chain), 0, head.last_stamp)
            }
        };
        self.step = Step::Playing(Playing {
            member,
            chain,
            clock: start.clock,
            team: start.team,
            history,
            last_stamp,
        });
        StartEffect::Playing { discarded }
    }

    /// Watches the match `start` names, for a client that answered to watch: sim tick
    /// the client runs by `start.clock`. Whether it did.
    pub fn watch(&mut self, start: WatchStart) -> bool {
        let answered = match mem::replace(&mut self.step, Step::Left) {
            Step::Answered(answered) if matches!(self.player.role, JoinRole::Watch { .. }) => {
                answered
            }
            other => {
                self.step = other;
                return false;
            }
        };
        self.step = Step::Watching(Watching {
            member: answered.member,
            clock: start.clock,
        });
        true
    }

    /// Notes that the session went back to a save, on a local server, which alone loads one, its
    /// link kept: a client that plays, watches or answered ends its match, its chain's history
    /// kept, and answers the server's next offer as it answers on a new link; whether it did.
    /// Any other server loads no save, so its word would let it end a match unseen.
    pub fn load(&mut self, hosting: Hosting) -> bool {
        if hosting != Hosting::Local {
            return false;
        }
        let answered = match mem::replace(&mut self.step, Step::Left) {
            Step::Playing(playing) => Answered {
                member: playing.member,
                history: Some(playing.history),
            },
            Step::Watching(watching) => Answered {
                member: watching.member,
                history: None,
            },
            Step::Answered(answered) => answered,
            other => {
                self.step = other;
                return false;
            }
        };
        self.step = Step::Loaded(answered);
        true
    }

    /// Notes that the client's link failed at `now`, by its real clock: a client that knows the
    /// server's times tries again, its chain's history kept; one that does not stops; one that
    /// left takes no receipt from then on.
    pub fn lose(&mut self, now: Duration, random: [u8; 32]) -> LinkLoss {
        let (member, history) = match mem::replace(&mut self.step, Step::Left) {
            Step::Playing(playing) => (playing.member, Some(playing.history)),
            Step::Answered(answered) | Step::Loaded(answered) => {
                (answered.member, answered.history)
            }
            Step::Watching(watching) => (watching.member, None),
            Step::Waiting => {
                self.step = Step::Lost(Loss::LinkFailed);
                return LinkLoss::Stopped(Loss::LinkFailed);
            }
            Step::Leaving(_) => return LinkLoss::Nothing,
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
            next_try: now + Self::wait(0, random),
        });
        LinkLoss::Rejoining
    }

    /// What a client trying its link again does at `now`, by its real clock: it connects after
    /// each wait, the next drawn with `random`, until the server's grace period and restore window
    /// passed since the link failed.
    pub fn retry(&mut self, now: Duration, random: [u8; 32]) -> Retry {
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
        rejoining.next_try = now + Self::wait(rejoining.tries, random);
        Retry::Connect
    }

    /// Takes `receipt`, while the player plays, or left and their link is still open, when the
    /// server key signed it over this player's chain as it stood at its seq, under their
    /// delegation or the one it renewed, no earlier seq than the one the client keeps, as a
    /// restored server gives the same head again: the client keeps it, and forgets its chain's
    /// history before it.
    pub fn take_receipt(
        &mut self,
        receipt: &SignedReceipt,
        verifier: &mut Verifier,
    ) -> Result<(), ReceiptRefusal> {
        let (member, player_slot, history) = match &mut self.step {
            Step::Playing(playing) => (
                &mut playing.member,
                playing.chain.player_slot(),
                &mut playing.history,
            ),
            Step::Leaving(leaving) => (
                &mut leaving.member,
                leaving.player_slot,
                &mut leaving.history,
            ),
            _ => return Err(ReceiptRefusal::NotPlaying),
        };
        let signed = receipt.receipt;
        if !verifier.verifies(&receipt.signature, &self.player.server.key, &signed) {
            return Err(ReceiptRefusal::BadSignature);
        }
        if signed.session_id != member.session.id
            || signed.player_slot != player_slot
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
        if history.head_at(next_seq) != Some(signed.head) {
            return Err(ReceiptRefusal::OtherHead);
        }
        history.forget_before(next_seq);
        member.receipt = Some(*receipt);
        Ok(())
    }

    /// The newest receipt the client keeps, from its first answer until it stops.
    pub const fn receipt(&self) -> Option<&SignedReceipt> {
        let member = match &self.step {
            Step::Answered(Answered { member, .. })
            | Step::Loaded(Answered { member, .. })
            | Step::Playing(Playing { member, .. })
            | Step::Watching(Watching { member, .. })
            | Step::Leaving(Leaving { member, .. })
            | Step::Rejoining(Rejoining { member, .. }) => member,
            Step::Waiting | Step::Refused(_) | Step::Left | Step::Lost(_) => return None,
        };
        member.receipt.as_ref()
    }

    /// The server dismissed the client, for `dismissed`: it stops.
    pub fn dismiss(&mut self, dismissed: Dismissed) {
        self.step = Step::Lost(match dismissed {
            Dismissed::Superseded => Loss::Superseded,
            Dismissed::JoinRefused => Loss::JoinRefused,
        });
    }

    /// The player leaves: the client tries no link again, and, when they played, takes the
    /// receipts that come until the link closes.
    pub fn leave(&mut self) {
        self.step = match mem::replace(&mut self.step, Step::Left) {
            Step::Playing(Playing {
                member,
                chain,
                history,
                ..
            }) => Step::Leaving(Leaving {
                member,
                player_slot: chain.player_slot(),
                history,
            }),
            _ => Step::Left,
        };
    }

    /// The wait before try `tries`, after the first, drawn at random below its bound, which
    /// starts at `FIRST_WAIT` and doubles to `LONGEST_WAIT`, as AWS's "full jitter" does: clients
    /// that lost one server do not all come back in one instant.
    fn wait(tries: u32, random: [u8; 32]) -> Duration {
        let bound = FIRST_WAIT * (1 << tries.min(DOUBLINGS));
        let draw = u64::from_le_bytes(random[..8].try_into().expect("8 bytes"));
        let nanos = (bound.as_nanos() * u128::from(draw)) >> 64;
        Duration::from_nanos(u64::try_from(nanos).expect("below the bound"))
    }
}

impl Playing {
    /// Extends the player's chain with the input of `payload` stamped `stamp`, noting its head.
    pub fn extend(&mut self, stamp: Tick, payload: &[u8]) {
        self.chain.extend(stamp, payload);
        self.history.push(self.chain.head());
        self.last_stamp = Some(stamp);
    }

    /// The stamp of an input sent as the client predicts tick `now`: `now`, or the last stamp
    /// when the client's timeline went back past it, as the network stack steps it back after the
    /// server's tick fell behind, since the log refuses a stamp that goes back.
    pub fn stamp(&self, now: Tick) -> Tick {
        self.last_stamp.map_or(now, |last| last.max(now))
    }
}

impl Joining {
    /// Whether `terms` name the server and rate the player means to reach, and a session of the
    /// client's mode, whose rules admit them.
    fn fits(&self, terms: &SessionTerms) -> Result<(), TermsMismatch> {
        if terms.server_key != self.server.key {
            return Err(TermsMismatch::OtherServer);
        }
        SessionRules::of(&self.packages)
            .check(terms)
            .map_err(TermsMismatch::Terms)?;
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
            seed_contribution: SeedContribution::new(signer.entropy().random()),
            expiration: now + DELEGATION_LIFETIME,
        };
        Delegation::sign(&self.main, &granted, now)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::num::NonZeroU32;
    use std::sync::Arc;

    use campfire_net_core::TlsKey;
    use campfire_package::ModePackages;
    use campfire_protocol::internals::TestKey;
    use campfire_protocol::{Entropy, Signer};

    use crate::client::join_state::{JoinState, Joining};
    use crate::client::server_pin::ServerPin;
    use crate::messages::join::JoinRole;

    impl JoinState {
        /// A client of `packages`, main key 1, that means to play on the remote server of key 8
        /// at `tick_hz`, and waits for its offer, as a client stands before it joins.
        pub fn waiting(packages: Arc<ModePackages>, tick_hz: NonZeroU32) -> Self {
            let server = ServerPin {
                key: TestKey::of(8).x_only_public_key().0,
                tls_key: TlsKey::new([3; 32]),
                tick_hz,
            };
            Self::new(Joining {
                main: Arc::new(Signer::new(TestKey::of(1), Entropy::Fixed(6))),
                server,
                role: JoinRole::Play,
                packages,
            })
        }
    }
}

#[cfg(test)]
mod tests;
