use std::num::NonZeroU32;

use campfire_common::PlayerSlot;
use campfire_log::internals::LogCheck;
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_protocol::{
    Checkpoint, Delegation, DelegationTerms, InputChain, InputPlace, SeedChain, ServerInput,
    ServerSeed, ServerSeeds, SessionHeader, SessionId, SessionLog, SessionResult, SessionTerms,
    Signature, SlotPlan, SlotStart,
};

use crate::harness::fixed_match::FixedMatch;
use crate::input_rules::InputRules;
use crate::runner::Runner;
use crate::session_rules::SessionRules;

/// A session of a mode's packages for tests and checks, whose server and players hold fixed keys:
/// every run signs alike, so every run of the same inputs gives the same log and state. The
/// player of slot `n` holds the same keys whether the session starts them in it or they join it
/// later.
#[derive(Debug)]
pub struct FixedSession {
    packages: ModePackages,
    terms: SessionTerms,
}

/// The seed chain of every fixed session, of room for a few checkpoints.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::new(4).unwrap());
/// The server's key, which signs its inputs.
fn server_keypair() -> Keypair {
    FixedSession::key(8)
}

fn server_key() -> XOnlyPublicKey {
    server_keypair().x_only_public_key().0
}

/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];
/// The time every delegation is signed at, and how long after it expires.
const NOW: u64 = 1_700_000_000;
const LIFETIME: u64 = 86_400;

impl FixedSession {
    /// A session of `packages` at `tick_hz` for `players` players, with roomy input limits.
    pub fn new(packages: ModePackages, tick_hz: NonZeroU32, players: u32) -> FixedSession {
        FixedSession::with_rules(packages, tick_hz, players, InputRules::ROOMY)
    }

    /// A session of `packages` at `tick_hz` for `players` players, its inputs within `rules`.
    pub fn with_rules(
        packages: ModePackages,
        tick_hz: NonZeroU32,
        players: u32,
        rules: InputRules,
    ) -> FixedSession {
        let plan = vec![SlotPlan::Player; usize::try_from(players).expect("players fit usize")];
        FixedSession::planned(packages, tick_hz, rules, plan)
    }

    /// A session of `packages` at `tick_hz`, its inputs within `rules`, its slots as `plan` opens
    /// them.
    pub fn planned(
        packages: ModePackages,
        tick_hz: NonZeroU32,
        rules: InputRules,
        plan: Vec<SlotPlan>,
    ) -> FixedSession {
        let terms = SessionRules::of(&packages)
            .terms(server_key(), SEED_CHAIN.commitment(), tick_hz, rules, plan)
            .unwrap_or_else(|error| panic!("{error}"));
        FixedSession { packages, terms }
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    /// The slots the session plays, whoever controls them.
    pub fn slots(&self) -> u32 {
        u32::try_from(self.terms.slots.len()).expect("the terms count slots in u32")
    }

    /// The session's terms.
    pub const fn terms(&self) -> &SessionTerms {
        &self.terms
    }

    /// The header of a session of `terms` with a start for each slot `terms` plans, each
    /// player's delegation signed for `terms`.
    pub fn header(&self, terms: SessionTerms) -> SessionHeader {
        let slots = (0..)
            .zip(&terms.slots)
            .map(|(slot, plan)| match plan {
                SlotPlan::Player => SlotStart::player(FixedSession::delegation(&terms, slot)),
                SlotPlan::Bot => SlotStart::Bot,
                SlotPlan::Open => SlotStart::Open,
            })
            .collect();
        SessionHeader { terms, slots }
    }

    /// The log of the session, as it starts, its seed not yet revealed.
    pub fn log(&self) -> SessionLog {
        SessionLog::new(self.header(self.terms.clone())).unwrap_or_else(|error| panic!("{error}"))
    }

    /// The seed of the log's segment `segment`.
    pub fn seed(segment: u32) -> ServerSeed {
        SEED_CHAIN.seed(segment)
    }

    /// Every segment's seed, as the server knows them.
    pub const fn seeds() -> ServerSeeds {
        SEED_CHAIN.seeds()
    }

    /// A match at tick 0, its players joined, none of their inputs sent yet.
    pub fn start(&self) -> FixedMatch {
        let check = LogCheck::start();
        let chains = (0..self.slots())
            .map(|slot| FixedSession::chain(&self.terms, slot))
            .collect();
        let runner = Runner::new(self.log(), FixedSession::seeds(), &self.packages)
            .unwrap_or_else(|error| panic!("{error}"));
        FixedMatch::new(runner, chains, self.terms.clone(), check)
    }

    /// The server's signature of `input` at `place` in the session of `session_id`.
    pub fn server_signature(
        input: &ServerInput<'_>,
        session_id: SessionId,
        place: InputPlace,
    ) -> Signature {
        input.sign(
            &Secp256k1::new(),
            &server_keypair(),
            session_id,
            place,
            &AUX,
        )
    }

    /// The server's signature of the checkpoint `record` in the session of `session_id`.
    pub fn checkpoint_signature(record: &Checkpoint, session_id: SessionId) -> Signature {
        record.sign(&Secp256k1::new(), &server_keypair(), session_id, &AUX)
    }

    /// The server's signature of `result` in the session of `session_id`.
    pub fn result_signature(result: &SessionResult, session_id: SessionId) -> Signature {
        result.sign(&Secp256k1::new(), &server_keypair(), session_id, &AUX)
    }

    /// Player `slot`'s chain, as they start it in a session of `terms`, from its first input.
    pub(crate) fn chain(terms: &SessionTerms, slot: u32) -> InputChain {
        let root = FixedSession::delegation(terms, slot).chain_root();
        InputChain::new(PlayerSlot::new(slot), root)
    }

    /// Player `slot`'s session key.
    pub(crate) fn session_key(slot: u32) -> Keypair {
        FixedSession::key(20 + slot)
    }

    /// The signature of every player's input packets, with no auxiliary randomness.
    pub(crate) const AUX: [u8; 32] = AUX;

    fn key(byte: u32) -> Keypair {
        let secret = SecretKey::from_byte_array(&[u8::try_from(byte).unwrap(); 32]).unwrap();
        Keypair::from_secret_key(&Secp256k1::new(), &secret)
    }

    /// Player `slot`'s delegation in a session of `terms`.
    pub(crate) fn delegation(terms: &SessionTerms, slot: u32) -> Delegation {
        let delegated = DelegationTerms {
            session_key: FixedSession::session_key(slot).x_only_public_key().0,
            server_key: server_key(),
            session_id: terms.session_id(),
            seed_contribution: [u8::try_from(slot).unwrap(); 32],
            expiration: NOW + LIFETIME,
        };
        Delegation::sign(
            &Secp256k1::new(),
            &FixedSession::key(10 + slot),
            &delegated,
            NOW,
            &AUX,
        )
    }
}
