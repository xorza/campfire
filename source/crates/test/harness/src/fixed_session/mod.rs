use std::iter;
use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_common::{DeclaredName, PlayerSlot};
use campfire_log::internals::LogCheck;
use campfire_mode::{HostPicks, OptionPicks, PlayerSlotPick, PlayerSlotPicks};
use campfire_package::ModePackages;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, InputRules, PlayerSlotPlan, PlayerSlotStart,
    SeedChain, SeedContribution, ServerSeed, ServerSeeds, SessionHeader, SessionLog, SessionTerms,
};
use campfire_runner::{HostChoices, Session, SessionRules, SessionSource, UncheckedSource};
use secp256k1::XOnlyPublicKey;

use crate::fixed_session::fixed_match::FixedMatch;

pub(crate) mod fixed_match;

/// A session of a mode's packages for tests and checks, whose server and players hold fixed keys:
/// every run signs alike, so every run of the same inputs gives the same log and state. The player
/// of player slot `n` holds the same keys whether the session starts them in it or they join it
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
    /// A session of `packages` at its manifest's default rate for `players` players, under the
    /// LAN's input limits, which the client plays under.
    pub fn new(packages: ModePackages, players: u32) -> Self {
        let players = usize::try_from(players).expect("players fit usize");
        Self::planned(packages, iter::repeat_n(PlayerSlotPlan::Player, players))
    }

    /// A session of `packages` at its manifest's default rate, under the LAN's input limits, its
    /// player slots as `plan` opens them.
    pub fn planned(packages: ModePackages, plan: impl IntoIterator<Item = PlayerSlotPlan>) -> Self {
        let tick_hz = packages.manifest().tick_hz.default();
        let session_rules = SessionRules::of(&packages);
        let visibility = session_rules.visibility().settings();
        let player_slots: Vec<PlayerSlotPlan> = plan.into_iter().collect();
        let picks = HostPicks::defaults(packages.data(), player_slots.len());
        let terms = session_rules
            .terms(HostChoices {
                server_key: server_key(),
                seed_commitment: SEED_CHAIN.commitment(),
                tick_hz,
                inputs: InputRules::LAN,
                player_slots,
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

    /// The session, its inputs within `rules`.
    #[must_use]
    pub fn rules(mut self, rules: InputRules) -> Self {
        self.terms.inputs = rules;
        SessionRules::of(&self.packages)
            .check(&self.terms)
            .expect("input rules the mode takes");
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

    /// The session, of `picks` of its player slots' choices of values, as its host picks them
    /// (08-sessions.md, The terms hold each player slot's picks).
    #[must_use]
    pub fn player_slot_picked(mut self, picks: &[PlayerSlotPick]) -> Self {
        let choices = &self.packages.data().choices;
        let player_slots = self.terms.player_slots.len();
        let picks = PlayerSlotPicks::named(choices, player_slots, picks)
            .expect("picks of the player slots' choices");
        self.terms.player_slot_picks = picks.places().to_vec();
        self
    }

    /// The session whose every player slot picks `value` of its choice `choice`, as its host
    /// picks them.
    #[must_use]
    pub fn each_picked(self, choice: &str, value: &str) -> Self {
        let choice = DeclaredName::new(choice).expect("a choice's name");
        let value = DeclaredName::new(value).expect("a value's name");
        let picks: Vec<PlayerSlotPick> = (0..self.player_slots())
            .map(|player_slot| PlayerSlotPick {
                player_slot: PlayerSlot::new(player_slot),
                choice: choice.clone(),
                value: value.clone(),
            })
            .collect();
        self.player_slot_picked(&picks)
    }

    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }

    /// The player slots the session plays, whoever controls them.
    pub fn player_slots(&self) -> u32 {
        u32::try_from(self.terms.player_slots.len()).expect("the terms count player slots in u32")
    }

    /// The session's terms.
    pub const fn terms(&self) -> &SessionTerms {
        &self.terms
    }

    /// The header of a session of `terms` with a start for each player slot `terms` plans, each
    /// player's delegation signed for `terms`.
    pub fn header(&self, terms: SessionTerms) -> SessionHeader {
        let player_slots = (0..)
            .zip(&terms.player_slots)
            .map(|(player_slot, plan)| match plan {
                PlayerSlotPlan::Player => {
                    PlayerSlotStart::player(Self::delegation(&terms, player_slot))
                }
                PlayerSlotPlan::Bot => PlayerSlotStart::Bot,
                PlayerSlotPlan::Open => PlayerSlotStart::Open,
            })
            .collect();
        SessionHeader {
            terms,
            player_slots,
        }
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
        let chains = (0..self.player_slots())
            .map(|player_slot| Self::chain(&self.terms, player_slot))
            .collect();
        let signers = TestKey::players()[..usize::try_from(self.player_slots()).unwrap()]
            .iter()
            .map(|keys| TestKey::signer(keys.session))
            .collect();
        let game = Session::start_match(self.source(self.log())).unwrap();
        FixedMatch::new(game, chains, signers, self.terms.clone(), log)
    }

    /// The server's source of a match of `log`, a log of this session: its seeds, its packages
    /// and the server's signer, as a server starts or resumes the match.
    pub fn source(&self, log: SessionLog) -> SessionSource<'_> {
        SessionSource::check(UncheckedSource {
            log,
            seeds: self.seeds(),
            packages: &self.packages,
            signer: Some(Arc::new(TestKey::server_signer())),
        })
        .unwrap()
    }

    /// Player `player_slot`'s chain, as they start it in a session of `terms`, from its first
    /// input.
    pub(crate) fn chain(terms: &SessionTerms, player_slot: u32) -> InputChain {
        let root = Self::delegation(terms, player_slot).chain_root();
        InputChain::new(PlayerSlot::new(player_slot), root)
    }

    /// Player `player_slot`'s delegation in a session of `terms`.
    pub(crate) fn delegation(terms: &SessionTerms, player_slot: u32) -> Delegation {
        let keys = TestKey::player(player_slot);
        let delegated = DelegationTerms {
            session_key: keys.session.x_only_public_key().0,
            server_key: server_key(),
            session_id: terms.session_id(),
            seed_contribution: SeedContribution::new([u8::try_from(player_slot).unwrap(); 32]),
            expiration: TestKey::NOW + LIFETIME,
        };
        Delegation::sign(&TestKey::signer(keys.main), &delegated, TestKey::NOW)
    }
}
