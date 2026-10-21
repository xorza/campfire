use std::ops::{Deref, DerefMut};

use bevy_ecs::world::World;
use campfire_declarations::MatchFacts;
use campfire_sim::StateRegistry;

use crate::books::Books;
use crate::release::Release;
use crate::release::match_kind::MatchKind;
use crate::release::match_setup::MatchSetup;

/// A match in a world of its own, which campfire holds wherever no `App` must: a test's, a
/// bench's, a verifier's. It is its world, whose traits give its operations: the release's build,
/// the sim's ticks and state, the players. A server's and a client's match is the world of their
/// `App`, built the same way.
#[derive(Debug)]
pub struct Match {
    world: World,
    /// A test's check of the warnings its match logs: last, so it drops after the world and sees
    /// what the world logs as it drops.
    #[cfg(feature = "internals")]
    #[expect(
        clippy::absolute_paths,
        reason = "a gated field names its type in full, as no gated import sits mid-file"
    )]
    _log: Option<campfire_log::internals::LogCheck>,
}

impl Match {
    /// The match of `spec`, of `books`, of `kind`, built in a new world.
    pub fn new(setup: MatchSetup, books: Books, kind: MatchKind<'_>) -> Self {
        let mut world = World::new();
        Release::build(&mut world, setup, books, kind);
        Self {
            world,
            #[cfg(feature = "internals")]
            _log: None,
        }
    }

    /// The match in `world`, which `Release::build` built, as a session's start
    /// builds it from its log.
    pub fn from_built(world: World) -> Self {
        assert!(
            world.contains_resource::<StateRegistry>() && world.contains_resource::<MatchFacts>(),
            "a match is a world the release built"
        );
        Self {
            world,
            #[cfg(feature = "internals")]
            _log: None,
        }
    }
}

impl Deref for Match {
    type Target = World;

    fn deref(&self) -> &World {
        &self.world
    }
}

impl DerefMut for Match {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::num::NonZeroU32;
    use std::sync::Arc;

    use bevy_ecs::world::World;
    use campfire_actions::{SlotKindBook, SlotKindData, SlotKinds};
    use campfire_combat::Combat;
    use campfire_common::{DeclaredName, SegmentSeed};
    use campfire_declarations::Role;
    use campfire_declarations::internals::BookLoad;
    use campfire_log::internals::LogCheck;
    use campfire_mode::{HostPicks, ModeSetup};
    use campfire_players::Players;
    use campfire_script::{ListPlace, ModeScripts, ScriptId, ScriptLimits, ScriptPools};
    use campfire_sim::{Capability, MatchRate};
    use campfire_units::PoolId;

    use crate::fixtures::package_set::PackageSet;
    use crate::release::Release;
    use crate::release::client_holds::ClientHolds;
    use crate::release::r#match::Match;
    use crate::release::match_kind::{MatchKind, ModeStart, PackageScript};
    use crate::release::match_setup::MatchSetup;
    use crate::release::predicted_parts::PredictedParts;

    impl Match {
        /// The match, which fails the test that holds it on any warning `log`, started before
        /// the match's build, takes.
        #[must_use]
        pub fn with_log(self, log: LogCheck) -> Self {
            let Self { world, _log: None } = self else {
                panic!("a match holds one log check");
            };
            Self {
                world,
                _log: Some(log),
            }
        }

        /// The match of the mode of `set`, loaded and built as a session's load and build make
        /// one, its books by `Release::books` and its scripts compiled, its mode installed and
        /// not started, as `start` gives. It fails the test on any warning it logs.
        pub fn of_package(set: &PackageSet, start: PackageStart) -> Self {
            let log = LogCheck::start();
            let PackageStart {
                players,
                limits,
                picks,
            } = start;
            let loaded = set.load(limits);
            let books = loaded.books(set);
            let pools = ScriptPools { limits, players };
            let capabilities = Release::capability_set(&set.capabilities).unwrap();
            let setup =
                MatchSetup::new(SegmentSeed::ZERO, set.rate, Role::Server(pools)).set(capabilities);
            let scripts: Vec<PackageScript<'_>> = (loaded.paths.iter())
                .zip(&set.scripts)
                .map(|(path, source)| PackageScript { path, source })
                .collect();
            let ids = (0..scripts.len()).map(ScriptId::nth);
            let mode = ModeSetup {
                scripts: ModeScripts::new(ids, ListPlace::new(set.own).unwrap()),
                data: &loaded.data,
                picks: &picks,
                teams: &set.teams,
                players,
            };
            let start = ModeStart {
                scripts: &scripts,
                setup: mode,
            };
            Self::new(setup, books, MatchKind::Mode(start)).with_log(log)
        }

        /// The MOBA's 30 ticks a second.
        pub const TEST_RATE: MatchRate = MatchRate::new(NonZeroU32::new(30).unwrap());

        /// A test's match of the capabilities `declared` and each capability they need, which the
        /// builder's setters shape: by default a server's at `TEST_RATE` that runs scripts within
        /// `ScriptLimits::ROOMY` for one player, started with no mode, as a start spawns the
        /// match's entity. With combat, its life pool is the first. It fails the test on any
        /// warning it logs.
        pub fn builder(declared: &[Capability]) -> MatchBuilder {
            MatchBuilder {
                declared: declared.to_vec(),
                rate: Self::TEST_RATE,
                role: Role::Server(ScriptPools {
                    limits: ScriptLimits::ROOMY,
                    players: 1,
                }),
                start: Start::WithoutMode,
            }
        }

        /// The match of `setup`, as `MatchBuilder::build` gives it before a client's parts and
        /// the start.
        fn test_of(setup: MatchSetup) -> Self {
            let log = LogCheck::start();
            let set = setup.set;
            let books = Release::empty_books(setup.rate());
            let mut world = World::new();
            Release::build(&mut world, setup.open(), books, MatchKind::Rules);
            // A mode declares the kinds its units' slots are of: here one, whose one rank comes
            // with the spawn, which a test that needs others replaces.
            let basic = SlotKindData {
                name: DeclaredName::new("basic").unwrap(),
                ranks: None,
            };
            let kinds = SlotKinds::new([basic]).unwrap();
            SlotKindBook(Arc::new(kinds)).load_into(&mut world);
            if set.contains(Capability::Combat) {
                Combat::bind_life(&mut world, PoolId::FIRST);
            }
            Self {
                world,
                _log: Some(log),
            }
        }
    }

    /// How a test's match of a package set starts: its players, its scripts' limits, and what
    /// the session's host picked of the mode's options and its seats' choices.
    #[derive(Debug)]
    pub struct PackageStart {
        pub players: u32,
        pub limits: ScriptLimits,
        pub picks: HostPicks,
    }

    /// How a test's match starts.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Start {
        /// With the match's entity and no player, as a start with no mode spawns it.
        WithoutMode,
        /// Not yet, for a mode or the test to start it.
        Later,
    }

    /// A test's match before its build, which `Match::builder` starts.
    #[derive(Debug)]
    pub struct MatchBuilder {
        declared: Vec<Capability>,
        rate: MatchRate,
        role: Role,
        start: Start,
    }

    impl MatchBuilder {
        /// At `rate`.
        #[must_use]
        pub const fn rate(mut self, rate: MatchRate) -> Self {
            self.rate = rate;
            self
        }

        /// Of `role`: `Role::Rules` runs no scripts, and `Role::Client` predicts, each of its
        /// sim's queries skipping the units it holds as the server sent them, and each unit it
        /// predicts taking the server's parts its owner's client receives, as replication brings
        /// them, which a client's spawn does not give.
        #[must_use]
        pub const fn role(mut self, role: Role) -> Self {
            self.role = role;
            self
        }

        /// With scripts for `players` players.
        #[must_use]
        pub fn players(mut self, players: u32) -> Self {
            self.pools().players = players;
            self
        }

        /// With scripts within `limits`.
        #[must_use]
        pub fn limits(mut self, limits: ScriptLimits) -> Self {
            self.pools().limits = limits;
            self
        }

        /// Unstarted, with no entity yet, for a mode or the test to start.
        #[must_use]
        pub const fn unstarted(mut self) -> Self {
            self.start = Start::Later;
            self
        }

        fn pools(&mut self) -> &mut ScriptPools {
            let Role::Server(pools) = &mut self.role else {
                panic!("only a server's match runs scripts");
            };
            pools
        }

        /// The match.
        pub fn build(self) -> Match {
            let set =
                Release::capability_set(&self.with_needs()).expect("a test declares a valid set");
            let client = matches!(self.role, Role::Client);
            let setup = MatchSetup::new(SegmentSeed::ZERO, self.rate, self.role).set(set);
            let mut game = Match::test_of(setup);
            if client {
                let holds = ClientHolds::of(&mut game, set);
                let world = &mut *game;
                Release::declare(
                    set,
                    &mut PredictedParts {
                        world,
                        holds: &holds,
                    },
                );
            }
            if self.start == Start::WithoutMode {
                Players::start_without_mode(&mut game, [], 0);
            }
            game
        }

        /// The capabilities declared, with each one they need, theirs in turn, in the order
        /// first named.
        fn with_needs(&self) -> Vec<Capability> {
            let rows = Release::rows();
            let mut declared = Vec::new();
            let mut next = self.declared.clone();
            next.reverse();
            while let Some(capability) = next.pop() {
                if declared.contains(&capability) {
                    continue;
                }
                declared.push(capability);
                let needs = rows
                    .needs(capability)
                    .expect("every capability has a module of the release");
                next.extend(needs.iter().rev().filter(|need| !declared.contains(need)));
            }
            declared
        }
    }
}
