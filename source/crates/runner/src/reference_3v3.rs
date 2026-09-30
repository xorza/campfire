use std::num::NonZeroU32;
use std::path::Path;

use campfire_capabilities::{InputValue, ModeInput};
use campfire_math::PlayerSlot;
use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, SeedChain, ServerSeed, SessionHeader, SessionLog,
    SessionTerms,
};

use crate::runner::Runner;
use crate::session::Session;

/// The reference 3v3 as its packages hold it, at its slowest rate, which plays a match in the
/// fewest ticks, with six players of fixed keys.
#[derive(Debug)]
pub struct Reference3v3 {
    packages: ModePackages,
}

const MODE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba/modes/3v3");
const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const SERVER_KEY: [u8; 32] = [8; 32];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];
/// The hero each slot picks.
const HEROES: [&str; 6] = [
    "hero-cinder",
    "hero-gale",
    "hero-husk",
    "hero-kensho",
    "hero-rime",
    "hero-veil",
];

impl Reference3v3 {
    pub const PLAYERS: u32 = 6;

    pub fn load() -> Reference3v3 {
        let packages =
            ModePackages::from_dir(Path::new(MODE)).unwrap_or_else(|error| panic!("{error}"));
        Reference3v3 { packages }
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    /// The seed of the log's first segment.
    pub fn seed() -> ServerSeed {
        SEED_CHAIN.seed(0)
    }

    /// A match at tick 0, in which each player picked a hero, in slot order, and two spells.
    pub fn start(&self) -> Runner {
        let terms = self.terms();
        let players = (0..Reference3v3::PLAYERS)
            .map(|slot| delegation(slot, &terms))
            .collect();
        let log = SessionLog::new(SessionHeader {
            terms: terms.clone(),
            players,
        })
        .unwrap();
        let mut runner = Runner::new(log, Reference3v3::seed(), &self.packages)
            .unwrap_or_else(|error| panic!("{error}"));
        let secp = Secp256k1::new();
        let mut applied = Vec::new();
        for (slot, hero) in (0..).zip(HEROES) {
            let mut chain =
                InputChain::new(PlayerSlot::new(slot), delegation(slot, &terms).chain_root());
            let payload = ModeInput::payload(&[
                ModeInput {
                    name: "hero",
                    value: InputValue::String(hero),
                },
                ModeInput {
                    name: "spells",
                    value: InputValue::StringList(vec!["haste", "mend"]),
                },
            ]);
            let input = chain.extend(0, &payload);
            let signature = chain.sign(&secp, &session_key(slot), terms.session_id(), &AUX);
            runner.record([input], &signature, &mut applied).unwrap();
        }
        runner
    }

    fn terms(&self) -> SessionTerms {
        SessionTerms {
            server_key: SERVER_KEY,
            tick_hz: TICK_HZ,
            max_input_delay: 10,
            max_input_lead: 10,
            max_payload_len: 256,
            max_inputs_per_tick: 4,
            seed_commitment: SEED_CHAIN.commitment(),
            release: RELEASE.to_owned(),
            mode: Session::mode_in_terms(&self.packages),
            dependencies: Session::dependencies_in_terms(&self.packages),
        }
    }
}

fn key(byte: u32) -> Keypair {
    let secret = SecretKey::from_byte_array(&[u8::try_from(byte).unwrap(); 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// Player `slot`'s session key.
fn session_key(slot: u32) -> Keypair {
    key(20 + slot)
}

/// Player `slot`'s delegation in the session of `terms`.
fn delegation(slot: u32, terms: &SessionTerms) -> Delegation {
    let delegated = DelegationTerms {
        session_key: session_key(slot).x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: terms.session_id(),
        seed_contribution: [u8::try_from(slot).unwrap(); 32],
        expiration: 1_700_086_400,
    };
    Delegation::sign(
        &Secp256k1::new(),
        &key(10 + slot),
        &delegated,
        1_700_000_000,
        &AUX,
    )
}
