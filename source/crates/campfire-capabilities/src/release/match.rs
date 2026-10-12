use std::ops::{Deref, DerefMut};

use bevy_ecs::world::World;
#[cfg(feature = "internals")]
use campfire_log::internals::LogCheck;
use campfire_sim::StateRegistry;

use crate::declare::capability_set::CapabilitySet;
use crate::release::Release;
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
    _log: Option<LogCheck>,
}

impl Match {
    /// The match of `spec`, built in a new world.
    pub fn new(spec: MatchSpec) -> Match {
        let mut world = World::new();
        Release::build(&mut world, spec);
        Match {
            world,
            #[cfg(feature = "internals")]
            _log: None,
        }
    }

    /// The match in `world`, which `Release::build` built, as a session's start
    /// builds it from its log.
    pub fn from_built(world: World) -> Match {
        assert!(
            world.contains_resource::<StateRegistry>()
                && world.contains_resource::<CapabilitySet>(),
            "a match is a world the release built"
        );
        Match {
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
    use campfire_log::internals::LogCheck;

    use crate::release::r#match::Match;

    impl Match {
        /// The match, which fails the test that holds it on any warning `log`, started before
        /// the match's build, takes.
        #[must_use]
        pub fn with_log(self, log: LogCheck) -> Match {
            let Match { world, _log: None } = self else {
                panic!("a match holds one log check");
            };
            Match {
                world,
                _log: Some(log),
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_matches {
    use std::num::NonZeroU32;

    use bevy_ecs::world::World;
    use campfire_codec::Binary;
    use campfire_common::SegmentSeed;
    use campfire_log::internals::LogCheck;
    use campfire_sim::{Capability, TickRate};
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::actions::slot_kind_data::SlotKindData;
    use crate::actions::slot_kinds::SlotKinds;
    use crate::combat::Combat;
    use crate::players::Players;
    use crate::release::Release;
    use crate::release::r#match::Match;
    use crate::release::match_spec::MatchSpec;
    use crate::scripts::script_book::ScriptBook;
    use crate::scripts::script_budgets::ScriptBudgets;
    use crate::stats::pool_id::PoolId;
    use crate::values::declared_name::DeclaredName;

    impl Match {
        /// The MOBA's 30 ticks a second.
        pub(crate) const TEST_RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

        /// A test's match at `rate` of the capabilities `declared`, running scripts within
        /// `budgets`, or none, as a client; with combat, its life pool the first. It has no mode
        /// to start it, so it starts with the match's entity, as a start spawns it. It fails the
        /// test on any warning it logs.
        pub(crate) fn test(
            declared: &[Capability],
            rate: TickRate,
            budgets: Option<ScriptBudgets>,
        ) -> Match {
            let mut started = Match::test_unstarted(declared, rate, budgets);
            Players::start_without_mode(&mut started, [], 0);
            started
        }

        /// The match `test` gives, with no entity yet, for a mode to start.
        pub(crate) fn test_unstarted(
            declared: &[Capability],
            rate: TickRate,
            budgets: Option<ScriptBudgets>,
        ) -> Match {
            let set = Release::capability_set(declared).expect("a test declares a valid set");
            let spec = MatchSpec::new(SegmentSeed::new([0; 32]), rate).capabilities(set);
            Match::test_of(match budgets {
                Some(budgets) => spec.script_budgets(budgets),
                None => spec,
            })
        }

        /// The match of `spec`, as `test_unstarted` gives it.
        fn test_of(spec: MatchSpec) -> Match {
            let log = LogCheck::start();
            let set = spec.set;
            let mut world = World::new();
            Release::build(&mut world, spec);
            // A match installs the book its package load read; a test fills one as it compiles.
            world.insert_resource(ScriptBook::default());
            // A mode declares the kinds its units' slots are of: here one, whose one rank comes
            // with the spawn, which a test that needs others replaces.
            if world.contains_resource::<SlotKinds>() {
                let basic = SlotKindData {
                    name: DeclaredName::new("basic").unwrap(),
                    ranks: None,
                };
                world.insert_resource(SlotKinds::from(vec![basic]));
            }
            if set.contains(Capability::Combat) {
                Combat::bind_life(&mut world, PoolId::FIRST);
            }
            Match {
                world,
                _log: Some(log),
            }
        }

        /// A client's match at `TEST_RATE` of `declared`: no script host, and no system that
        /// runs scripts.
        pub(crate) fn client(declared: &[Capability]) -> Match {
            Match::test(declared, Match::TEST_RATE, None)
        }

        /// A predicting client's match at `TEST_RATE` of `declared`, as `client` gives it: each
        /// of its sim's queries skips the units it holds as the server sent them, `Unpredicted`.
        pub(crate) fn predicting(declared: &[Capability]) -> Match {
            let set = Release::capability_set(declared).expect("a test declares a valid set");
            let spec = MatchSpec::new(SegmentSeed::new([0; 32]), Match::TEST_RATE)
                .capabilities(set)
                .predicting();
            let mut started = Match::test_of(spec);
            Players::start_without_mode(&mut started, [], 0);
            started
        }

        /// A server's match at `TEST_RATE` of `declared`, running scripts within `budgets`.
        pub(crate) fn server(declared: &[Capability], budgets: ScriptBudgets) -> Match {
            Match::test(declared, Match::TEST_RATE, Some(budgets))
        }

        /// Whether `value`, encoded as a snapshot holds it, decodes again: a decode keeps the
        /// rules a type's own values follow, as a restore check keeps those that need the match.
        pub(crate) fn decodes<T: Serialize + DeserializeOwned>(value: &T) -> bool {
            Binary::decode::<T>(&Binary::encode(value)).is_ok()
        }
    }
}
