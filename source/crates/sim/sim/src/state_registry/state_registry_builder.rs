use bevy_ecs::world::World;

use crate::host_rule::HostRule;
use crate::id_allocator::IdAllocator;
use crate::position::Position;
use crate::sim_state::{SimComponent, SimResource};
use crate::sim_tick::SimTick;
use crate::state_registry::breach::Breach;
use crate::state_registry::{Entry, ForeignCheck, StateRegistry, holders_hold, holders_hosted};

/// The state types a registry will hold, as the installs register them: `build` fixes them in a
/// `StateRegistry`, which takes no more.
#[derive(Debug, Clone)]
pub struct StateRegistryBuilder {
    entries: Vec<Entry>,
    foreign: Vec<ForeignCheck>,
    /// Whether a record of a world's changes holds its types, each by its place, which a type
    /// registered later would move.
    recorded: bool,
}

impl StateRegistryBuilder {
    /// A builder with the sim's own state: the entity list, the id allocator, the tick and
    /// positions.
    pub(crate) fn new() -> Self {
        let mut registry = Self {
            entries: Vec::new(),
            foreign: Vec::new(),
            recorded: false,
        };
        let entities = Entry::entities();
        registry.register(entities);
        registry.register_resource::<IdAllocator>();
        registry.register_resource::<SimTick>();
        registry.register_component::<Position>();
        registry
    }

    pub fn register_component<C: SimComponent>(&mut self) {
        self.register(Entry::component::<C>());
    }

    /// Requires that every entity holding `H` holds `C`, both registered types, as a restore
    /// checks: a part a kind's entities all hold, or one another part of an entity always comes
    /// with. A snapshot that breaks it fails as `SnapshotError::Lacks`.
    pub fn require<H: SimComponent, C: SimComponent>(&mut self) {
        self.assert_registered::<C>();
        self.foreign.push(ForeignCheck::Shape(holders_hold::<H, C>));
    }

    /// Requires that `C`, a registered type of a crate that knows no kinds, sits on the entities
    /// of `H` alone, as its own `Host` cannot say. A snapshot that breaks it fails as
    /// `SnapshotError::WrongHost`.
    pub fn host<C: SimComponent, H: HostRule>(&mut self) {
        self.assert_registered::<C>();
        self.foreign
            .push(ForeignCheck::Shape(holders_hosted::<C, H>));
    }

    /// Adds `check` to the restore checks of `C`, a registered type: a rule of it that a
    /// capability other than its owner knows, which `C::check` cannot see. A breach on an entity
    /// fails as `SnapshotError::Invalid` of that entity; one of the whole match, of none.
    pub fn add_check<C: SimComponent>(&mut self, check: fn(&World) -> Result<(), Breach>) {
        self.assert_registered::<C>();
        self.foreign.push(ForeignCheck::Rule {
            name: C::NAME,
            check,
        });
    }

    fn assert_registered<C: SimComponent>(&self) {
        assert!(
            self.entries.iter().any(|entry| entry.name == C::NAME),
            "{} is registered before a check is added to it",
            C::NAME
        );
    }

    /// Registers `R` as a resource every state holds, as the sim's own are: the match's install
    /// inserts it, and a snapshot that records it absent does not restore, so no system finds it
    /// missing.
    pub fn register_resource<R: SimResource>(&mut self) {
        self.register(Entry::resource::<R>());
    }

    /// Whether `C` is registered.
    pub fn holds<C: SimComponent>(&self) -> bool {
        self.entries
            .binary_search_by(|held| held.name.cmp(C::NAME))
            .is_ok()
    }

    /// The registry of the types registered.
    pub fn build(self) -> StateRegistry {
        StateRegistry {
            entries: self.entries,
            foreign: self.foreign,
        }
    }

    fn register(&mut self, entry: Entry) {
        let name = entry.name;
        assert!(
            !self.recorded,
            "state type {name:?} registered after its world's changes are tracked"
        );
        let Err(at) = self.entries.binary_search_by(|held| held.name.cmp(name)) else {
            panic!("state type {name:?} registered twice");
        };
        self.entries.insert(at, entry);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::state_registry::StateRegistry;
    use crate::state_registry::state_registry_builder::StateRegistryBuilder;

    impl StateRegistryBuilder {
        /// The types of `registry`, to register more, as a test's open build or an install after
        /// the build takes them; none once `recorded`, as a record of the changes of its world
        /// holds them.
        pub(crate) fn reopened(registry: StateRegistry, recorded: bool) -> Self {
            Self {
                entries: registry.entries,
                foreign: registry.foreign,
                recorded,
            }
        }
    }
}
