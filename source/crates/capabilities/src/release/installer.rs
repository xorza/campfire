use std::any::type_name;
use std::fmt::{self, Debug};

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;
use campfire_declare::{
    Book, Cache, Callbacks, CapabilitySet, ColumnFill, Declarations, EntityKind, EntityParts,
    KindSpawn, KindVisitor, ModuleSet, Report, Scratch, SpawnParts,
};
use campfire_script::{ApiBuilder, CallPart, Ctx, RowReads, ViewColumn};
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource, StateRegistry};
use campfire_state_types::{Replication, StateTypes};
use campfire_units::{EntityKinds, ViewReader};
use campfire_values::EngineTag;

/// The consumer that puts a module's declarations into a match: its state types and restore
/// checks into the state registry, its system sets into the schedule, and the rest into the
/// world.
pub(super) struct Installer<'a> {
    world: &'a mut World,
    schedule: &'a mut Schedule,
    registry: &'a mut StateRegistry,
    set: CapabilitySet,
    scripts: bool,
    /// The capability of the module whose declarations it installs.
    capability: Option<Capability>,
}

impl<'a> Installer<'a> {
    /// Installs into `world`, `schedule` and `registry`, of a match of the capabilities `set` that
    /// runs scripts when `scripts` says.
    pub(super) const fn new(
        world: &'a mut World,
        schedule: &'a mut Schedule,
        registry: &'a mut StateRegistry,
        set: CapabilitySet,
        scripts: bool,
    ) -> Installer<'a> {
        Installer {
            world,
            schedule,
            registry,
            set,
            scripts,
            capability: None,
        }
    }
}

impl Debug for Installer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installer")
            .field("set", &self.set)
            .field("scripts", &self.scripts)
            .field("capability", &self.capability)
            .finish_non_exhaustive()
    }
}

impl StateTypes for Installer<'_> {
    fn component<C: Replication>(&mut self) {
        self.registry.component::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {
        self.registry.resource::<R>();
        self.world.insert_resource(R::default());
    }

    fn sim_component<C: Replication>(&mut self) {
        self.registry.sim_component::<C>();
    }
}

impl KindVisitor for Installer<'_> {
    fn kind<K: EntityKind>(&mut self) {
        let id = self.world.register_component::<K>();
        self.world.get_resource_or_init::<EntityKinds>().add(id);
    }
}

impl Declarations for Installer<'_> {
    fn module(&mut self, capability: Option<Capability>) {
        self.capability = capability;
    }

    fn require<H: SimComponent, C: SimComponent>(&mut self) {
        self.registry.require::<H, C>();
    }

    fn host<C: SimComponent, H: HostRule>(&mut self) {
        self.registry.host::<C, H>();
    }

    fn check<C: SimComponent>(&mut self, check: fn(&World) -> Result<(), Breach>) {
        self.registry.add_check::<C>(check);
    }

    fn book<B: Book>(&mut self) {
        if B::held(self.set, self.scripts) {
            self.world.insert_resource(B::default());
        }
    }

    fn sets<S: ModuleSet>(&mut self) {
        assert_eq!(
            S::CAPABILITY,
            self.capability,
            "the sets of {} are their owner's to declare",
            type_name::<S>()
        );
        for &set in S::ALL {
            self.schedule.configure_sets(set.in_set(set.place().set()));
        }
    }

    fn scratch<R: Scratch>(&mut self) {
        self.world.insert_resource(R::default());
    }

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {
        self.world.init_resource::<C>();
    }

    fn callback<C: Callbacks>(&mut self, key: C::Key, callback: C::Callback) {
        assert_eq!(
            C::owner(key),
            self.capability,
            "the callback of {key:?} is its owner's to declare"
        );
        self.world
            .get_resource_or_init::<C>()
            .register(key, callback);
    }

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, fill: ColumnFill<D, C>) {
        ViewReader::add_source::<D, C>(self.world, fill);
    }

    fn call_part<P: CallPart + Default>(&mut self) {
        if self.scripts {
            self.world.non_send::<Ctx>().frame().add_part(P::default());
        }
    }

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        start: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
        let id = self.world.register_component::<C>();
        self.world.init_resource::<SpawnParts>();
        self.world
            .get_resource_or_init::<EntityParts<H>>()
            .add(id, start);
        self.registry.add_check::<C>(EntityParts::<H>::check::<C>);
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
