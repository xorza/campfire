use std::iter;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::PlayerSlot;
use campfire_log::internals::LogCheck;
use campfire_mode::OptionPicks;
use campfire_package::ModePackages;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, InputRules, SeedChain, SeedContribution, ServerSeed,
    ServerSeeds, SessionHeader, SessionLog, SessionTerms, Signer, SlotPlan, SlotStart,
};
use campfire_runner::{HostChoices, Session, SessionRules, SessionSource};
use secp256k1::XOnlyPublicKey;

use crate::fixed_session::fixed_match::FixedMatch;

pub(crate) mod fixed_match;

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
const SEED_CHAIN: SeedChain = TestKey::seed_chain(NonZeroU32::new(4).unwrap());
fn server_key() -> XOnlyPublicKey {
    TestKey::server().x_only_public_key().0
}

/// How long after `TestKey::NOW` every delegation expires.
const LIFETIME: u64 = 86_400;

impl FixedSession {
    /// A session of `packages` at `tick_hz` for `players` players, with roomy input limits.
    pub fn new(packages: ModePackages, tick_hz: NonZeroU32, players: u32) -> Self {
        Self::with_rules(packages, tick_hz, players, InputRules::ROOMY)
    }

    /// A session of `packages` at `tick_hz` for `players` players, its inputs within `rules`.
    pub fn with_rules(
        packages: ModePackages,
        tick_hz: NonZeroU32,
        players: u32,
        rules: InputRules,
    ) -> Self {
        let players = usize::try_from(players).expect("players fit usize");
        let plan = iter::repeat_n(SlotPlan::Player, players);
        Self::planned(packages, tick_hz, rules, plan)
    }

    /// A session of `packages` at `tick_hz`, its inputs within `rules`, its slots as `plan` opens
    /// them.
    pub fn planned(
        packages: ModePackages,
        tick_hz: NonZeroU32,
        rules: InputRules,
        plan: impl IntoIterator<Item = SlotPlan>,
    ) -> Self {
        let session_rules = SessionRules::of(&packages);
        let visibility = session_rules.visibility().settings();
        let picks = OptionPicks::defaults(session_rules.options());
        let terms = session_rules
            .terms(HostChoices {
                server_key: server_key(),
                seed_commitment: SEED_CHAIN.commitment(),
                tick_hz,
                inputs: rules,
                slots: plan.into_iter().collect(),
                visibility,
                picks: &picks,
            })
            .unwrap();
        Self { packages, terms }
    }

    /// The session, of the values at `places` among its mode's options' values picked, as its
    /// host picks them (10-sessions.md, The host picks, and the terms record the pick).
    #[must_use]
    pub fn picked(mut self, places: &[u8]) -> Self {
        let rules = SessionRules::of(&self.packages);
        let picks = OptionPicks::checked(rules.options(), places).expect("picks of the options");
        self.terms.options = picks.places().to_vec();
        self
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
                SlotPlan::Player => SlotStart::player(Self::delegation(&terms, slot)),
                SlotPlan::Bot => SlotStart::Bot,
                SlotPlan::Open => SlotStart::Open,
            })
            .collect();
        SessionHeader { terms, slots }
    }

    /// The log of the session, as it starts, its seed not yet revealed.
    pub fn log(&self) -> SessionLog {
        SessionLog::new(self.header(self.terms.clone())).unwrap()
    }

    /// The seed of the log's segment `segment`.
    pub fn seed(segment: u32) -> ServerSeed {
        SEED_CHAIN.seed(segment)
    }

    /// Every segment's seed, as the server knows them.
    pub fn seeds() -> ServerSeeds {
        SEED_CHAIN.seeds()
    }

    /// A match at tick 0, its players joined, none of their inputs sent yet.
    pub fn start(&self) -> FixedMatch {
        let log = LogCheck::start();
        let chains = (0..self.slots())
            .map(|slot| Self::chain(&self.terms, slot))
            .collect();
        let signers = TestKey::players()[..usize::try_from(self.slots()).unwrap()]
            .iter()
            .map(|keys| TestKey::signer(keys.session))
            .collect();
        let source = SessionSource {
            log: self.log(),
            seeds: Self::seeds(),
            packages: &self.packages,
            signer: Some(Arc::new(Self::server_signer())),
        };
        let game = Session::start_match(source).unwrap();
        FixedMatch::new(game, chains, signers, self.terms.clone(), log)
    }

    /// The signer of the tests' server key, whose auxiliary randomness is fixed.
    pub fn server_signer() -> Signer {
        TestKey::signer(TestKey::server())
    }

    /// Player `slot`'s chain, as they start it in a session of `terms`, from its first input.
    pub(crate) fn chain(terms: &SessionTerms, slot: u32) -> InputChain {
        let root = Self::delegation(terms, slot).chain_root();
        InputChain::new(PlayerSlot::new(slot), root)
    }

    /// Player `slot`'s delegation in a session of `terms`.
    pub(crate) fn delegation(terms: &SessionTerms, slot: u32) -> Delegation {
        let keys = TestKey::player(slot);
        let delegated = DelegationTerms {
            session_key: keys.session.x_only_public_key().0,
            server_key: server_key(),
            session_id: terms.session_id(),
            seed_contribution: SeedContribution::new([u8::try_from(slot).unwrap(); 32]),
            expiration: TestKey::NOW + LIFETIME,
        };
        Delegation::sign(&TestKey::signer(keys.main), &delegated, TestKey::NOW)
    }
}
