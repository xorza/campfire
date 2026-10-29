use std::any::TypeId;
use std::ops::Range;
use std::sync::LazyLock;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::EventPattern;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::SystemCondition;
#[cfg(feature = "internals")]
use bevy_ecs::schedule::{InternedSystemSet, SystemSet};
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_declarations::{
    Book, BuildFn, Cache, Callbacks, CapabilitySet, ColumnFill, Declarations, EntityKind, ExtendFn,
    Fact, KindSpawn, KindVisitor, Listed, Module, ModuleSystems, ModuleVisitor, NamesFn, OneShot,
    Report, Role, RuleFn, Scratch, Section, SectionReaders,
};
use campfire_script::{
    ApiBuilder, CallExtension, CallParams, EventPhase, Hook, HookDecl, ModeEvents,
    PlannedCapability, RowReads, Status, ViewColumn,
};
use campfire_sim::{
    Breach, Capability, HostRule, OwnResources, SimComponent, SimResource, TimedProcess,
};
use campfire_states::{Replication, StateTypes};
use campfire_values::{EngineTag, MatchScope, NameId, Named, Names};

use crate::release::Release;
use crate::release::declaring::ScriptedPart;

static CATALOG: LazyLock<ReleaseCatalog> = LazyLock::new(ReleaseCatalog::walk);

/// What the release's modules declare that holds whatever a match's capabilities, gathered by one
/// walk of the module list: each module, and the hooks, engine tags, sections, registries of
/// callbacks and system sets they declare, in the order they declare them.
#[derive(Debug, Default)]
pub(super) struct ReleaseCatalog {
    modules: Vec<CatalogModule>,
    hooks: Vec<Hook>,
    engine_tags: Vec<EngineTag>,
    sections: SectionReaders,
    called: Vec<TypeId>,
    #[cfg(feature = "internals")]
    sets: Vec<InternedSystemSet>,
}

/// A module of the list: its capability, none for the core's, what it builds on, whether it is
/// the mode's scripted part, and its runs of the catalog's registries and sets.
#[derive(Debug, Clone)]
struct CatalogModule {
    capability: Option<Capability>,
    needs: &'static [Capability],
    scripted: bool,
    called: Range<usize>,
    #[cfg(feature = "internals")]
    sets: Range<usize>,
}

impl ReleaseCatalog {
    /// The catalog of the release, walked once.
    pub(super) fn get() -> &'static Self {
        &CATALOG
    }

    fn walk() -> Self {
        let mut catalog = Self::default();
        Release::modules(&mut catalog);
        for (at, tag) in catalog.engine_tags.iter().enumerate() {
            assert_eq!(
                tag.tag().index(),
                at,
                "{} takes its place among the engine tags",
                tag.name()
            );
        }
        catalog
    }

    /// Every capability of the list, the core's with them.
    pub(super) fn capabilities(&self) -> CapabilitySet {
        self.rows()
            .fold(CapabilitySet::CORE, |set, (capability, _)| {
                set.with(capability)
            })
    }

    /// What `capability` builds on; none for the mode, which every match has.
    pub(super) fn needs(&self, capability: Capability) -> Option<&'static [Capability]> {
        self.rows()
            .find(|&(row, _)| row == capability)
            .map(|(_, needs)| needs)
    }

    /// Each capability of the list but the mode, which a mode does not declare, with what it
    /// builds on.
    pub(super) fn rows(&self) -> impl Iterator<Item = (Capability, &'static [Capability])> {
        self.modules
            .iter()
            .filter(|module| !module.scripted)
            .filter_map(|module| Some((module.capability?, module.needs)))
            .filter(|&(capability, _)| capability != Capability::Mode)
    }

    pub(super) fn hooks(&self) -> &[Hook] {
        &self.hooks
    }

    /// In the order of their numbers, which `walk` asserts.
    pub(super) fn engine_tags(&self) -> &[EngineTag] {
        &self.engine_tags
    }

    pub(super) const fn sections(&self) -> &SectionReaders {
        &self.sections
    }

    /// The modules a match of `set` runs, the mode's scripted part as `scripts` says.
    fn running(
        &self,
        set: CapabilitySet,
        scripts: ScriptedPart,
    ) -> impl Iterator<Item = &CatalogModule> {
        self.modules.iter().filter(move |module| {
            if module.scripted {
                scripts == ScriptedPart::Visited
            } else {
                set.runs(module.capability)
            }
        })
    }

    /// The registries of callbacks the modules of a match of `set` call or declare callbacks in.
    pub(super) fn called(&self, set: CapabilitySet, scripts: ScriptedPart) -> Vec<TypeId> {
        self.running(set, scripts)
            .flat_map(|module| &self.called[module.called.clone()])
            .copied()
            .collect()
    }

    /// The system sets the modules of a match of `set` declare, its mode's scripted part's among
    /// them.
    #[cfg(feature = "internals")]
    pub(super) fn sets(&self, set: CapabilitySet) -> Vec<InternedSystemSet> {
        self.running(set, ScriptedPart::Visited)
            .flat_map(|module| &self.sets[module.sets.clone()])
            .copied()
            .collect()
    }

    fn visit<M: Module>(&mut self, scripted: bool) {
        self.modules.push(CatalogModule {
            capability: M::CAPABILITY,
            needs: M::NEEDS,
            scripted,
            called: self.called.len()..self.called.len(),
            #[cfg(feature = "internals")]
            sets: self.sets.len()..self.sets.len(),
        });
        self.sets::<M::Systems>();
        M::declare(self);
        let module = self.modules.last_mut().unwrap();
        module.called.end = self.called.len();
        #[cfg(feature = "internals")]
        {
            module.sets.end = self.sets.len();
        }
    }
}

impl ModuleVisitor for ReleaseCatalog {
    fn module<M: Module>(&mut self) {
        self.visit::<M>(false);
    }

    fn scripted<M: Module>(&mut self) {
        self.visit::<M>(true);
    }
}

impl StateTypes for ReleaseCatalog {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for ReleaseCatalog {
    fn kind<K: EntityKind>(&mut self) {}

    fn state<K: EntityKind, S: Component>(&mut self) {}
}

impl Declarations for ReleaseCatalog {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self, _: BuildFn<B>) {}

    fn fact<F: Fact>(&mut self, _: BuildFn<F>) {}

    fn names<I: NameId + Named + 'static>(&mut self, _: NamesFn<I>)
    where
        Names<I>: Send + Sync,
        I::Scope: MatchScope,
    {
    }

    fn extend<B: Send + Sync + 'static>(&mut self, _: ExtendFn<B>) {}

    fn rule(&mut self, _: RuleFn) {}

    fn section<S: Section>(&mut self) {
        let module = self.modules.last().expect("a section is a module's");
        self.sections.add::<S>(module.capability);
    }

    fn listed<L: Listed>(&mut self) {
        self.sections.add_listed::<L>();
    }

    fn sets<S: ModuleSystems>(&mut self) {
        #[cfg(feature = "internals")]
        self.sets.extend(S::ALL.iter().map(SystemSet::intern));
    }

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn timed<P: TimedProcess>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn mode_events<Q: ModeEvents>(&mut self, _: EventPhase) {}

    fn calls<C: Callbacks>(&mut self) {
        self.called.push(TypeId::of::<C>());
    }

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {
        self.called.push(TypeId::of::<C>());
    }

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_extension<P: CallExtension + Default>(&mut self) {}

    fn call_params<P: CallParams + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
    }

    fn engine_tags(&mut self, tags: &'static [EngineTag]) {
        self.engine_tags.extend_from_slice(tags);
    }

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}

    fn hook<H: HookDecl>(&mut self, _: Status) {
        self.hooks.push(Hook::of::<H>());
    }

    fn plan_hook<H: HookDecl>(&mut self, _: PlannedCapability) {
        self.hooks.push(Hook::of::<H>());
    }

    fn observer<E: EventPattern, M>(&mut self, _: impl IntoObserverSystem<E, M>) {}

    fn removals<C: Component>(&mut self) {}

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, R::Out, M> + 'static) {}

    fn loaded<R: Resource>(&mut self) {}

    fn package_data<R: Resource + Default>(&mut self) {}
    fn stages_run_if<M>(&mut self, _: impl SystemCondition<M> + Clone) {}

    fn server_part<H: Component, C: Component + Default>(&mut self) {}

    fn part_with<H: Component, C: Component + Default>(&mut self) {}

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> Option<R>) {}
}

impl OwnResources for ReleaseCatalog {
    fn own<R: Resource>(&mut self) {}
}
