use std::any::type_name;
use std::fmt::{self, Debug};

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemCondition};
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_declare::{
    Book, BuildFn, BuiltBooks, Cache, Callbacks, CapabilitySet, ColumnFill, Declarations,
    EntityKind, EntityParts, ExtendFn, Fact, InstallFacts, KindSpawn, KindVisitor, Listed,
    ModuleSet, OneShot, Report, Role, RuleFn, Scratch, Section, SpawnParts,
};
use campfire_script::{
    ApiBuilder, CallParams, CallPart, Ctx, HookSpec, PlannedCapability, RowReads, Status,
    ViewColumn,
};
use campfire_sim::{
    Breach, Capability, DueIndex, HostRule, OwnResources, Removals, SimComponent, SimEdge,
    SimResource, SimSet, StateRegistryBuilder, TimedProcess, TimedRuns,
};
use campfire_state_types::{Replication, StateTypes};
use campfire_units::{EntityKinds, ViewReader};
use campfire_values::EngineTag;

use crate::release::stage_span::StageSpan;

/// The consumer that puts a module's declarations into a match: its state types and restore
/// checks into the state registry, its system sets into the schedule, its books from the built
/// ones, and the rest into the world.
pub(super) struct Installer<'a> {
    world: &'a mut World,
    schedule: &'a mut Schedule,
    registry: &'a mut StateRegistryBuilder,
    books: &'a mut BuiltBooks,
    set: CapabilitySet,
    role: &'a Role,
    /// The capability of the module whose declarations it installs.
    capability: Option<Capability>,
}

/// What an install fills: the world, the schedule and the registry of the match, and the books
/// it takes its modules' from.
pub(super) struct InstallParts<'a> {
    pub(super) world: &'a mut World,
    pub(super) schedule: &'a mut Schedule,
    pub(super) registry: &'a mut StateRegistryBuilder,
    pub(super) books: &'a mut BuiltBooks,
}

impl<'a> Installer<'a> {
    /// Installs into the match of `parts`, as `facts` says, its books from `books`.
    pub(super) const fn new(parts: InstallParts<'a>, facts: &'a InstallFacts<'a>) -> Self {
        let InstallParts {
            world,
            schedule,
            registry,
            books,
        } = parts;
        Installer {
            world,
            schedule,
            registry,
            books,
            set: facts.facts.set,
            role: &facts.facts.role,
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

    /// A role that does not compute the state holds none of its resources, which no server
    /// sends, so a system of it that reads one fails its parameter validation.
    fn resource<R: SimResource + Default>(&mut self) {
        if self.role.computes_state() {
            self.registry.resource::<R>();
            self.world.insert_resource(R::default());
        }
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

    fn fact<F: Fact>(&mut self, _: BuildFn<F>) {}

    fn extend<B: Send + Sync + 'static>(&mut self, _: ExtendFn<B>) {}

    fn rule(&mut self, _: RuleFn) {}

    /// The book its builder made, in place before its owner installs, so the install reads it;
    /// the script view takes what it copies of it.
    fn book<B: Book>(&mut self, _: BuildFn<B>) {
        let book = (self.books.take::<B>())
            .unwrap_or_else(|| panic!("{} is declared and built", type_name::<B>()));
        if B::held(self.role) {
            self.world.insert_resource(book);
            B::share_in(self.world);
        }
    }

    fn section<S: Section>(&mut self) {}

    fn listed<L: Listed>(&mut self) {}

    fn sets<S: ModuleSet>(&mut self) {
        for &set in S::ALL {
            set.place().configure(set, self.schedule);
        }
    }

    fn scratch<R: Scratch>(&mut self) {
        if let Some(scratch) = R::installed(self.role) {
            self.world.insert_resource(scratch);
        }
    }

    fn cache<R: Cache>(&mut self) {
        if let Some(cache) = R::installed(self.world) {
            self.world.insert_resource(cache);
        }
    }

    fn timed<P: TimedProcess>(&mut self) {
        if P::RUNS == TimedRuns::Server && !self.role.computes_state() {
            return;
        }
        self.world.insert_resource(DueIndex::<P>::default());
        DueIndex::<P>::track(self.world, self.schedule);
    }

    fn report<R: Report>(&mut self) {
        if let Some(report) = R::installed(self.role) {
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

    /// Only a match that runs scripts reads the script view's rows.
    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, fill: ColumnFill<D, C>) {
        if self.role.scripts() {
            ViewReader::add_source::<D, C>(self.world, fill);
        }
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

    fn hook<H: HookSpec>(&mut self, _: Status) {}

    fn plan_hook<H: HookSpec>(&mut self, _: PlannedCapability) {}

    fn observer<E: Event, B: Bundle, M>(&mut self, observer: impl IntoObserverSystem<E, B, M>) {
        self.world.add_observer(observer);
    }

    fn removals<C: Component>(&mut self) {
        Removals::<C>::track(self.world, self.schedule);
    }

    fn one_shot<R: OneShot, M>(&mut self, system: impl IntoSystem<R::In, R::Out, M> + 'static) {
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

    fn loaded<R: Resource>(&mut self) {}

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

impl OwnResources for Installer<'_> {
    /// The build puts it in, some as it ends, as the registry and the schedule.
    fn own<R: Resource>(&mut self) {}
}
