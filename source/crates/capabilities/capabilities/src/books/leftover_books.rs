use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::EventPattern;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::SystemCondition;
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_declarations::{
    Book, BuildFn, BuiltBooks, Cache, Callbacks, ColumnFill, Declarations, EntityKind, ExtendFn,
    Fact, KindSpawn, KindVisitor, Listed, ModuleSystems, OneShot, Report, Role, RuleFn, Scratch,
    Section,
};
use campfire_script::{
    ApiBuilder, CallExtension, CallParams, HookDecl, PlannedCapability, RowReads, Status,
    ViewColumn,
};
use campfire_sim::{
    Breach, Capability, HostRule, OwnResources, SimComponent, SimResource, TimedProcess,
};
use campfire_states::{Replication, StateTypes};
use campfire_values::EngineTag;

/// The end of the books' install, a walk of every module's declarations: each book of a module
/// the match does not run, which its install did not take, is dropped, so a built book no module
/// declares is left.
#[derive(Debug)]
pub(crate) struct LeftoverBooks<'a>(pub(crate) &'a mut BuiltBooks);

impl StateTypes for LeftoverBooks<'_> {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for LeftoverBooks<'_> {
    fn kind<K: EntityKind>(&mut self) {}

    fn state<K: EntityKind, S: Component>(&mut self) {}
}

impl Declarations for LeftoverBooks<'_> {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn fact<F: Fact>(&mut self, _: BuildFn<F>) {}

    fn extend<B: Send + Sync + 'static>(&mut self, _: ExtendFn<B>) {}

    fn rule(&mut self, _: RuleFn) {}

    fn book<B: Book>(&mut self, _: BuildFn<B>) {
        drop(self.0.take::<B>());
    }

    fn section<S: Section>(&mut self) {}

    fn listed<L: Listed>(&mut self) {}

    fn sets<S: ModuleSystems>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn timed<P: TimedProcess>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {}

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {}

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_extension<P: CallExtension + Default>(&mut self) {}

    fn call_params<P: CallParams + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}

    fn hook<H: HookDecl>(&mut self, _: Status) {}

    fn plan_hook<H: HookDecl>(&mut self, _: PlannedCapability) {}

    fn observer<E: EventPattern, M>(&mut self, _: impl IntoObserverSystem<E, M>) {}

    fn removals<C: Component>(&mut self) {}

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, R::Out, M> + 'static) {}

    fn loaded<R: Resource>(&mut self) {}

    fn stages_run_if<M>(&mut self, _: impl SystemCondition<M> + Clone) {}

    fn server_part<H: Component, C: Component + Default>(&mut self) {}

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> Option<R>) {}
}

impl OwnResources for LeftoverBooks<'_> {
    fn own<R: Resource>(&mut self) {}
}
