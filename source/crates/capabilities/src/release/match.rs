use std::ops::{Deref, DerefMut};

use bevy_ecs::world::World;
use campfire_declare::MatchFacts;
use campfire_sim::StateRegistry;

use crate::books::Books;
use crate::release::Release;
use crate::release::match_kind::MatchKind;
use crate::release::match_spec::MatchSpec;

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
    pub fn new(spec: MatchSpec, books: Books, kind: MatchKind<'_>) -> Self {
        let mut world = World::new();
        Release::build(&mut world, spec, books, kind);
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
    use campfire_codec::Binary;
    use campfire_combat::Combat;
    use campfire_common::{DeclaredName, SegmentSeed};
    use campfire_declare::{Book, Role};
    use campfire_log::internals::LogCheck;
    use campfire_mode::Mode;
    use campfire_players::Players;
    use campfire_script::ScriptPools;
    use campfire_sim::{Capability, TickRate};
    use campfire_stats::PoolId;
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::release::Release;
    use crate::release::r#match::Match;
    use crate::release::match_kind::MatchKind;
    use crate::release::match_spec::MatchSpec;

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
    }

    impl Match {
        /// The MOBA's 30 ticks a second.
        pub const TEST_RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

        /// A test's server match at `rate` of the capabilities `declared`, running scripts
        /// within `budgets`, or none; with combat, its life pool the first. It has no mode
        /// to start it, so it starts with the match's entity, as a start spawns it. It fails the
        /// test on any warning it logs.
        pub fn test(declared: &[Capability], rate: TickRate, pools: Option<ScriptPools>) -> Self {
            let mut started = Self::test_unstarted(declared, rate, pools);
            Players::start_without_mode(&mut started, [], 0);
            started
        }

        /// The match `test` gives, with no entity yet, for a mode to start.
        pub fn test_unstarted(
            declared: &[Capability],
            rate: TickRate,
            pools: Option<ScriptPools>,
        ) -> Self {
            let set = Release::capability_set(declared).expect("a test declares a valid set");
            let role = pools.map_or(Role::Rules, Role::Server);
            let spec = MatchSpec::new(SegmentSeed::ZERO, rate, role).set(set);
            Self::test_of(spec)
        }

        /// The match of `spec`, as `test_unstarted` gives it.
        fn test_of(spec: MatchSpec) -> Self {
            let log = LogCheck::start();
            let set = spec.set;
            let books = Release::empty_books(spec.rate());
            let mut world = World::new();
            Release::build(&mut world, spec.open(), books, MatchKind::Rules);
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

        /// A server's match at `TEST_RATE` of `declared` that runs no scripts: no script host,
        /// and no system that runs scripts, as a client's.
        pub fn unscripted(declared: &[Capability]) -> Self {
            Self::test(declared, Self::TEST_RATE, None)
        }

        /// A predicting client's match at `TEST_RATE` of `declared`: each of its sim's queries
        /// skips the units it holds as the server sent them, `Unpredicted`.
        pub fn predicting(declared: &[Capability]) -> Self {
            let set = Release::capability_set(declared).expect("a test declares a valid set");
            let spec = MatchSpec::new(SegmentSeed::ZERO, Self::TEST_RATE, Role::Client).set(set);
            let mut started = Self::test_of(spec);
            Mode::keep_no_spawner(&mut started);
            Players::start_without_mode(&mut started, [], 0);
            started
        }

        /// A server's match at `TEST_RATE` of `declared`, running scripts within `budgets`.
        pub fn server(declared: &[Capability], pools: ScriptPools) -> Self {
            Self::test(declared, Self::TEST_RATE, Some(pools))
        }

        /// Whether `value`, encoded as a snapshot holds it, decodes again: a decode keeps the
        /// rules a type's own values follow, as a restore check keeps those that need the match.
        pub fn decodes<T: Serialize + DeserializeOwned>(value: &T) -> bool {
            Binary::decode::<T>(&Binary::encode(value)).is_ok()
        }
    }
}
