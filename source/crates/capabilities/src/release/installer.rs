use std::any::type_name;
use std::fmt::{self, Debug};

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemCondition};
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_declare::{
    Book, Cache, Callbacks, CapabilitySet, ColumnFill, Declarations, EntityKind, EntityParts,
    KindSpawn, KindVisitor, MatchFacts, ModuleSet, OneShot, Report, Role, Scratch, SpawnParts,
};
use campfire_script::{ApiBuilder, CallParams, CallPart, Ctx, RowReads, ViewColumn};
use campfire_sim::{
    Breach, Capability, HostRule, Removals, SimComponent, SimEdge, SimResource, SimSet,
    StateRegistry,
};
use campfire_state_types::{Replication, StateTypes};
use campfire_units::{EntityKinds, ViewReader};
use campfire_values::EngineTag;

use crate::release::stage_span::StageSpan;

/// The consumer that puts a module's declarations into a match: its state types and restore
/// checks into the state registry, its system sets into the schedule, and the rest into the
/// world.
pub(super) struct Installer<'a> {
    world: &'a mut World,
    schedule: &'a mut Schedule,
    registry: &'a mut StateRegistry,
    set: CapabilitySet,
    role: &'a Role,
    /// The capability of the module whose declarations it installs.
    capability: Option<Capability>,
}

impl<'a> Installer<'a> {
    /// Installs into the match of `world` and `schedule`, as `facts` says, its state types into
    /// `registry`.
    pub(super) const fn new(
        world: &'a mut World,
        schedule: &'a mut Schedule,
        registry: &'a mut StateRegistry,
        facts: &'a MatchFacts<'a>,
    ) -> Self {
        Installer {
            world,
            schedule,
            registry,
            set: facts.set,
            role: facts.role,
            capability: None,
        }
    }
}

impl Debug for Installer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installer")
            .field("set", &self.set)
            .field("role", &self.role)
            .field("capability", &self.capability)
            .finish_non_exhaustive()
    }
}

impl StateTypes for Installer<'_> {
    fn component<C: Replication>(&mut self) {
        self.registry.component::<C>();
        self.world.register_component::<C>();
    }

    fn derived<C: Component>(&mut self) {
        self.world.register_component::<C>();
    }

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.world.register_component::<C>();
    }

    fn resource<R: SimResource + Default>(&mut self) {
        self.registry.resource::<R>();
        self.world.insert_resource(R::default());
    }

    fn sim_component<C: Replication>(&mut self) {
        self.registry.sim_component::<C>();
        self.world.register_component::<C>();
    }
}

impl KindVisitor for Installer<'_> {
    fn kind<K: EntityKind>(&mut self) {
        let id = self.world.register_component::<K>();
        self.world.get_resource_or_init::<EntityKinds>().add(id);
    }

    fn state<K: EntityKind, S: Component>(&mut self) {}
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
        if B::held(self.role) {
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

    fn cache<R: Cache>(&mut self) {
        if let Some(cache) = R::installed() {
            self.world.insert_resource(cache);
        }
    }

    fn report<R: Report>(&mut self) {
        if let Some(report) = R::installed() {
            self.world.insert_resource(report);
        }
    }

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
        if self.role.scripts() {
            self.world.non_send::<Ctx>().frame().add_part(P::default());
        }
    }

    fn call_params<P: CallParams + Default>(&mut self) {
        if self.role.scripts() {
            self.world
                .non_send::<Ctx>()
                .frame()
                .add_params(P::default());
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

    fn observer<E: Event, B: Bundle, M>(&mut self, observer: impl IntoObserverSystem<E, B, M>) {
        self.world.add_observer(observer);
    }

    fn removals<C: Component>(&mut self) {
        Removals::<C>::track(self.world, self.schedule);
    }

    fn one_shot<R: OneShot, M>(&mut self, system: impl IntoSystem<R::In, (), M> + 'static) {
        let id = self.world.register_system(system);
        self.world.insert_resource(R::new(id));
    }

    fn server_part<H: Component, C: Component + Default>(&mut self) {
        if self.role.computes_state() {
            self.world.register_required_components::<H, C>();
        }
    }

    fn on_insert<C: Component>(&mut self, hook: ComponentHook) {
        self.world.register_component_hooks::<C>().on_insert(hook);
    }

    fn non_send<R: 'static>(&mut self, build: fn(&World, &Role) -> Option<R>) {
        if let Some(resource) = build(self.world, self.role) {
            self.world.insert_non_send(resource);
        }
    }

    fn non_send_loaded<R: 'static>(&mut self) {}

    /// Each stage checks `condition` once, as it starts, with `SimEdge::Start` before them all,
    /// so the stage it turns false in is the last to run, its closing set with it.
    fn stages_run_if<M>(&mut self, condition: impl SystemCondition<M> + Clone) {
        self.schedule
            .configure_sets(SimEdge::Start.run_if(condition.clone()));
        for stage in SimSet::ALL {
            let span = StageSpan(stage);
            self.schedule.configure_sets((
                stage.in_set(span),
                SimEdge::After(stage).in_set(span),
                span.run_if(condition.clone()),
            ));
        }
    }
}
