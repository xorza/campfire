use std::num::NonZeroU32;

use campfire_math::PlayerSlot;
use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, SeedChain, ServerSeed, SessionHeader, SessionLog,
    SessionTerms,
};

use crate::fixed_match::FixedMatch;
use crate::runner::Runner;
use crate::session::Session;

/// A session of a mode's packages for tests and checks, whose server and players hold fixed keys:
/// every run signs alike, so every run of the same inputs gives the same log and state.
#[derive(Debug)]
pub struct FixedSession {
    packages: ModePackages,
    terms: SessionTerms,
    players: u32,
}

/// The seed chain of every fixed session.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const SERVER_KEY: [u8; 32] = [8; 32];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];
/// The time every delegation is signed at, and how long after it expires.
const NOW: u64 = 1_700_000_000;
const LIFETIME: u64 = 86_400;

impl FixedSession {
    /// A session of `packages` at `tick_hz` for `players` players, with roomy input limits.
    pub fn new(packages: ModePackages, tick_hz: NonZeroU32, players: u32) -> FixedSession {
        let terms = SessionTerms {
            server_key: SERVER_KEY,
            tick_hz,
            max_input_delay: 10,
            max_input_lead: 10,
            max_payload_len: 256,
            max_inputs_per_tick: 4,
            seed_commitment: SEED_CHAIN.commitment(),
            release: RELEASE.to_owned(),
            mode: Session::mode_in_terms(&packages),
            dependencies: Session::dependencies_in_terms(&packages),
        };
        FixedSession {
            packages,
            terms,
            players,
        }
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    pub const fn players(&self) -> u32 {
        self.players
    }

    /// The seed of the log's first segment.
    pub fn seed() -> ServerSeed {
        SEED_CHAIN.seed(0)
    }

    /// A match at tick 0, its players joined, none of their inputs sent yet.
    pub fn start(&self) -> FixedMatch {
        let players = (0..self.players)
            .map(|slot| self.delegation(slot))
            .collect();
        let log = SessionLog::new(SessionHeader {
            terms: self.terms.clone(),
            players,
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let runner = Runner::new(log, FixedSession::seed(), &self.packages)
            .unwrap_or_else(|error| panic!("{error}"));
        let chains = (0..self.players)
            .map(|slot| InputChain::new(PlayerSlot::new(slot), self.delegation(slot).chain_root()))
            .collect();
        FixedMatch::new(runner, chains, self.terms.session_id())
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

    /// Player `slot`'s delegation in this session.
    fn delegation(&self, slot: u32) -> Delegation {
        let delegated = DelegationTerms {
            session_key: FixedSession::session_key(slot).x_only_public_key().0,
            server_key: SERVER_KEY,
            session_id: self.terms.session_id(),
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
