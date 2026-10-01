use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::{Capability, StateRegistry};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::abilities::Abilities;
use crate::capability_set::error::CapabilityError;
use crate::combat::Combat;
use crate::mode::match_end::MatchEnd;
use crate::navigation::Navigation;
use crate::orders::Orders;
use crate::projectiles::Projectiles;
use crate::scripts::match_scripts::MatchScripts;
use crate::stats::Stats;
use crate::units::Units;
use crate::vision::Vision;

pub(crate) mod error;

/// A mode's declared capabilities: none twice, not `mode`, which every match has, and each with
/// the ones it builds on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet(u16);

/// A capability's install into a match.
type Install = fn(&mut World, &mut Schedule, &mut StateRegistry);

/// The capabilities the release runs, in the order they install: each after the ones it builds
/// on. A declared capability not here installs nothing yet.
const INSTALLS: [(Capability, Install); 7] = [
    (Capability::Stats, Stats::install),
    (Capability::Combat, Combat::install),
    (Capability::Navigation, Navigation::install),
    (Capability::Vision, Vision::install),
    (Capability::Projectiles, Projectiles::install),
    (Capability::Abilities, Abilities::install),
    (Capability::Orders, Orders::install),
];

impl CapabilitySet {
    /// The set of `declared`; an error when one is `mode`, one is declared twice, or one lacks a
    /// capability it builds on.
    pub fn new(declared: &[Capability]) -> Result<CapabilitySet, CapabilityError> {
        let mut set = CapabilitySet(0);
        for &capability in declared {
            if capability == Capability::Mode {
                return Err(CapabilityError::DeclaresMode);
            }
            if set.contains(capability) {
                return Err(CapabilityError::Repeated(capability));
            }
            set.0 |= bit(capability);
        }
        for capability in set.iter() {
            if let Some(&needs) = needs(capability)
                .iter()
                .find(|&&needs| !set.contains(needs))
            {
                return Err(CapabilityError::Needs { capability, needs });
            }
        }
        Ok(set)
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & bit(capability) != 0
    }

    /// The declared capabilities, in the order of their indices.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |&capability| self.contains(capability))
    }

    /// Installs the core, then each declared capability the release runs, each after the ones it
    /// builds on. A match that ended runs no stage. With no `scripts`, as on a client, which runs none, the core has no script host,
    /// and each capability leaves out what runs scripts.
    pub fn install(
        self,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        scripts: Option<MatchScripts>,
    ) {
        Units::install(world, schedule, registry, scripts);
        MatchEnd::stop_stages(schedule);
        for (capability, install) in INSTALLS {
            if self.contains(capability) {
                install(world, schedule, registry);
            }
        }
    }
}

/// A manifest's `capabilities`, checked as `CapabilitySet::new` checks them.
impl<'de> Deserialize<'de> for CapabilitySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CapabilitySet, D::Error> {
        let declared = Vec::<Capability>::deserialize(deserializer)?;
        CapabilitySet::new(&declared).map_err(D::Error::custom)
    }
}

const fn bit(capability: Capability) -> u16 {
    1 << capability as u16
}

/// The capabilities `capability` builds on.
const fn needs(capability: Capability) -> &'static [Capability] {
    match capability {
        Capability::Combat => &[Capability::Stats],
        Capability::Projectiles | Capability::Abilities | Capability::Vision => {
            &[Capability::Combat]
        }
        Capability::Orders => &[Capability::Combat, Capability::Navigation],
        _ => &[],
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::fmt;

    use campfire_math::SegmentSeed;
    use campfire_sim::{SimUpdate, TickRate};

    use super::*;

    /// A match for a capability's tests: a world that `SimUpdate::prepare` set up, with the core
    /// and the declared capabilities installed; and its schedule and state registry, for what a
    /// test installs or loads before the schedule goes into the world.
    pub(crate) struct TestMatch {
        pub(crate) world: World,
        pub(crate) schedule: Schedule,
        pub(crate) registry: StateRegistry,
    }

    /// A schedule prints nothing, so a match prints only what it is.
    impl fmt::Debug for TestMatch {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("TestMatch")
        }
    }

    impl TestMatch {
        /// A match at `rate` of the capabilities `declared`, running `scripts`.
        pub(crate) fn new(
            declared: &[Capability],
            rate: TickRate,
            scripts: Option<MatchScripts>,
        ) -> TestMatch {
            let mut world = World::new();
            SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
            let mut schedule = SimUpdate::schedule();
            let mut registry = StateRegistry::new();
            let set = CapabilitySet::new(declared).expect("a test declares a valid set");
            set.install(&mut world, &mut schedule, &mut registry, scripts);
            TestMatch {
                world,
                schedule,
                registry,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_script::ScriptHost;
    use campfire_sim::TickRate;

    use std::rc::Rc;

    use super::*;
    use crate::actions::action_book::ActionBook;
    use crate::capability_set::internals::TestMatch;
    use crate::orders::ai::Ai;
    use crate::scripts::script_limits::ScriptLimits;
    use crate::units::by_type::ByType;
    use crate::units::script_view::View;

    use Capability::{Abilities, Combat, Mode, Navigation, Orders, Projectiles, Stats, Vision};

    #[test]
    fn a_set_holds_each_capability_once_with_what_it_builds_on() {
        let set = CapabilitySet::new(&[Orders, Navigation, Stats, Combat, Vision]).unwrap();
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [Combat, Stats, Orders, Navigation, Vision]
        );
        assert!(set.contains(Orders) && !set.contains(Abilities));
        assert_eq!(CapabilitySet::new(&[]).unwrap().iter().count(), 0);
        for (declared, error) in [
            (&[Combat, Mode][..], CapabilityError::DeclaresMode),
            (
                &[Combat],
                CapabilityError::Needs {
                    capability: Combat,
                    needs: Stats,
                },
            ),
            (&[Combat, Vision, Combat], CapabilityError::Repeated(Combat)),
            (
                &[Projectiles],
                CapabilityError::Needs {
                    capability: Projectiles,
                    needs: Combat,
                },
            ),
            (
                &[Abilities],
                CapabilityError::Needs {
                    capability: Abilities,
                    needs: Combat,
                },
            ),
            (
                &[Stats, Combat, Orders],
                CapabilityError::Needs {
                    capability: Orders,
                    needs: Navigation,
                },
            ),
            (
                &[Navigation, Orders],
                CapabilityError::Needs {
                    capability: Orders,
                    needs: Combat,
                },
            ),
        ] {
            assert_eq!(CapabilitySet::new(declared), Err(error), "{declared:?}");
        }
    }

    /// Installs `declared` into a fresh match that runs `scripts`.
    fn installed(declared: &[Capability], scripts: Option<MatchScripts>) -> World {
        let rate = TickRate::new(NonZeroU32::new(30).unwrap());
        TestMatch::new(declared, rate, scripts).world
    }

    #[test]
    fn a_match_without_scripts_installs_no_host_or_ai_and_the_core_its_actions() {
        let all = [Stats, Combat, Navigation, Projectiles, Abilities, Orders];
        let scripts = MatchScripts {
            limits: ScriptLimits {
                per_call: 1,
                player: 1,
                think: 1,
                mode: 1,
            },
            players: 1,
            damage_kinds: Rc::from([]),
            stats: Rc::from([]),
            pools: Rc::from([]),
            resources: Rc::from([]),
        };
        let scripted = installed(&all, Some(scripts.clone()));
        let client = installed(&all, None);
        for (world, scripts) in [(&scripted, true), (&client, false)] {
            assert_eq!(world.get_non_send::<ScriptHost>().is_some(), scripts);
            assert!(world.contains_resource::<ActionBook>());
            assert_eq!(world.contains_resource::<ByType<Ai>>(), scripts);
            assert!(world.get_non_send::<View>().is_some());
        }
        let combat_only = installed(&[Stats, Combat], Some(scripts));
        assert!(combat_only.contains_resource::<ActionBook>());
    }
}
