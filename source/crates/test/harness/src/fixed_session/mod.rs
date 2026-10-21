use std::iter;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::Seat;
use campfire_log::internals::LogCheck;
use campfire_mode::{HostPicks, OptionPicks, SeatPick, SeatPicks};
use campfire_package::ModePackages;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, InputRules, SeatPlan, SeatStart, SeedChain,
    SeedContribution, ServerSeed, ServerSeeds, SessionHeader, SessionLog, SessionTerms, Signer,
};
use campfire_runner::{HostChoices, Session, SessionRules, SessionSource, UncheckedSource};
use secp256k1::XOnlyPublicKey;

use crate::fixed_session::fixed_match::FixedMatch;

pub(crate) mod fixed_match;

/// A session of a mode's packages for tests and checks, whose server and players hold fixed keys:
/// every run signs alike, so every run of the same inputs gives the same log and state. The
/// player of seat `n` holds the same keys whether the session starts them in it or they join it
/// later.
#[derive(Debug)]
pub struct FixedSession {
    packages: ModePackages,
    terms: SessionTerms,
    chain: SeedChain,
}

/// The seed chain of a fixed session, of room for a few checkpoints, which `segments` lengthens.
const SEED_CHAIN: SeedChain = TestKey::seed_chain(NonZeroU32::new(4).unwrap());
fn server_key() -> XOnlyPublicKey {
    TestKey::server().x_only_public_key().0
}

/// How long after `TestKey::NOW` every delegation expires.
const LIFETIME: u64 = 86_400;

impl FixedSession {
    /// A session of `packages` at its manifest's default rate for `players` players, with roomy
    /// input limits.
    pub fn new(packages: ModePackages, players: u32) -> Self {
        Self::with_rules(packages, players, InputRules::ROOMY)
    }

    /// A session of `packages` at its manifest's default rate for `players` players, its inputs
    /// within `rules`.
    pub fn with_rules(packages: ModePackages, players: u32, rules: InputRules) -> Self {
        let players = usize::try_from(players).expect("players fit usize");
        let plan = iter::repeat_n(SeatPlan::Player, players);
        Self::planned(packages, rules, plan)
    }

    /// A session of `packages` at its manifest's default rate, its inputs within `rules`, its
    /// seats as `plan` opens them.
    pub fn planned(
        packages: ModePackages,
        rules: InputRules,
        plan: impl IntoIterator<Item = SeatPlan>,
    ) -> Self {
        let tick_hz = packages.manifest().tick_hz.default();
        let session_rules = SessionRules::of(&packages);
        let visibility = session_rules.visibility().settings();
        let seats: Vec<SeatPlan> = plan.into_iter().collect();
        let picks = HostPicks::defaults(packages.data(), seats.len());
        let terms = session_rules
            .terms(HostChoices {
                server_key: server_key(),
                seed_commitment: SEED_CHAIN.commitment(),
                tick_hz,
                inputs: rules,
                seats,
                visibility,
                picks: &picks,
            })
            .unwrap();
        Self {
            packages,
            terms,
            chain: SEED_CHAIN,
        }
    }

    /// The session at `tick_hz` ticks a second, a rate its mode runs at.
    #[must_use]
    pub fn rate(mut self, tick_hz: NonZeroU32) -> Self {
        self.terms.tick_hz = tick_hz;
        let rules = SessionRules::of(&self.packages);
        rules.check(&self.terms).expect("a rate the mode runs at");
        self
    }

    /// The session on a seed chain of `segments` segments, for a match that checkpoints up to
    /// `segments` − 1 times. The chain's first seed, which the terms commit to, is another than
    /// the default chain's, so a match's draws change with it.
    #[must_use]
    pub fn segments(mut self, segments: NonZeroU32) -> Self {
        self.chain = TestKey::seed_chain(segments);
        self.terms.seed_commitment = self.chain.commitment();
        self
    }

    /// The session, of the values at `places` among its mode's options' values picked, as its
    /// host picks them (08-sessions.md, The host picks, and the terms record the pick).
    #[must_use]
    pub fn picked(mut self, places: &[u8]) -> Self {
        let rules = SessionRules::of(&self.packages);
        let picks = OptionPicks::checked(rules.options(), places).expect("picks of the options");
        self.terms.options = picks.places().to_vec();
        self
    }

    /// The session, of `picks` of its seats' choices of values, as its host picks them
    /// (08-sessions.md, The terms hold each seat's picks).
    #[must_use]
    pub fn seat_picked(mut self, picks: &[SeatPick]) -> Self {
        let choices = &self.packages.data().choices;
        let seats = self.terms.seats.len();
        let picks = SeatPicks::named(choices, seats, picks).expect("picks of the seats' choices");
        self.terms.seat_picks = picks.places().to_vec();
        self
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    /// The seats the session plays, whoever controls them.
    pub fn seats(&self) -> u32 {
        u32::try_from(self.terms.seats.len()).expect("the terms count seats in u32")
    }

    /// The session's terms.
    pub const fn terms(&self) -> &SessionTerms {
        &self.terms
    }

    /// The header of a session of `terms` with a start for each seat `terms` plans, each
    /// player's delegation signed for `terms`.
    pub fn header(&self, terms: SessionTerms) -> SessionHeader {
        let seats = (0..)
            .zip(&terms.seats)
            .map(|(seat, plan)| match plan {
                SeatPlan::Player => SeatStart::player(Self::delegation(&terms, seat)),
                SeatPlan::Bot => SeatStart::Bot,
                SeatPlan::Open => SeatStart::Open,
            })
            .collect();
        SessionHeader { terms, seats }
    }

    /// The log of the session, as it starts, its seed not yet revealed.
    pub fn log(&self) -> SessionLog {
        SessionLog::new(self.header(self.terms.clone())).unwrap()
    }

    /// The seed of the log's segment `segment`.
    pub fn seed(&self, segment: u32) -> ServerSeed {
        self.chain.seed(segment)
    }

    /// Every segment's seed, as the server knows them.
    pub fn seeds(&self) -> ServerSeeds {
        self.chain.seeds()
    }

    /// A match at tick 0, its players joined, none of their inputs sent yet.
    pub fn start(&self) -> FixedMatch {
        let log = LogCheck::start();
        let chains = (0..self.seats())
            .map(|seat| Self::chain(&self.terms, seat))
            .collect();
        let signers = TestKey::players()[..usize::try_from(self.seats()).unwrap()]
            .iter()
            .map(|keys| TestKey::signer(keys.session))
            .collect();
        let source = SessionSource::check(UncheckedSource {
            log: self.log(),
            seeds: self.seeds(),
            packages: &self.packages,
            signer: Some(Arc::new(Self::server_signer())),
        })
        .unwrap();
        let game = Session::start_match(source).unwrap();
        FixedMatch::new(game, chains, signers, self.terms.clone(), log)
    }

    /// The signer of the tests' server key, whose auxiliary randomness is fixed.
    pub fn server_signer() -> Signer {
        TestKey::signer(TestKey::server())
    }

    /// Player `seat`'s chain, as they start it in a session of `terms`, from its first input.
    pub(crate) fn chain(terms: &SessionTerms, seat: u32) -> InputChain {
        let root = Self::delegation(terms, seat).chain_root();
        InputChain::new(Seat::new(seat), root)
    }

    /// Player `seat`'s delegation in a session of `terms`.
    pub(crate) fn delegation(terms: &SessionTerms, seat: u32) -> Delegation {
        let keys = TestKey::player(seat);
        let delegated = DelegationTerms {
            session_key: keys.session.x_only_public_key().0,
            server_key: server_key(),
            session_id: terms.session_id(),
            seed_contribution: SeedContribution::new([u8::try_from(seat).unwrap(); 32]),
            expiration: TestKey::NOW + LIFETIME,
        };
        Delegation::sign(&TestKey::signer(keys.main), &delegated, TestKey::NOW)
    }
}
