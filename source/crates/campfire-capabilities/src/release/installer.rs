use std::any::type_name;
use std::fmt::{self, Debug};

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource, StateRegistry};

use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::entity_parts::EntityParts;
use crate::declare::kind_spawn::KindSpawn;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::module_set::ModuleSet;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::declare::set_order::{SetOrder, Side};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::scripts::ctx::Ctx;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::engine_tag::EngineTag;
use crate::units::entity_kind::EntityKind;
use crate::units::entity_kinds::EntityKinds;
use crate::units::row_reads::RowReads;
use crate::units::view::View;
use crate::units::view_column::ViewColumn;

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
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, declared: Declared) {
        self.registry.once::<C>(declared);
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(
        &mut self,
        declared: Declared,
    ) {
        self.registry.on_insert::<C>(declared);
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, declared: Declared) {
        self.registry.on_change::<C>(declared);
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        declared: Declared,
    ) {
        self.registry.predicted::<C>(declared);
    }

    fn server<C: Replication>(&mut self, declared: Declared) {
        self.registry.server::<C>(declared);
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
        let mut order = SetOrder::default();
        for &set in S::ALL {
            order.clear();
            set.order(&mut order);
            let mut config = set.in_set(set.place().set());
            for edge in &order {
                config = match edge.side {
                    Side::After => config.after(edge.set),
                    Side::Before => config.before(edge.set),
                };
            }
            self.schedule.configure_sets(config);
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
        let view = self.world.non_send::<View>().clone();
        view.add_column(C::default());
        view.add_source::<D, C>(self.world, fill);
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
        self.world
            .get_resource_or_init::<EntityParts<H>>()
            .add(start);
        self.registry.add_check::<C>(EntityParts::<H>::check::<C>);
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
