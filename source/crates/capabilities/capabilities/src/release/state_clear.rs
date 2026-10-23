use std::any::type_name;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::EventPattern;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::message::Messages;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::SystemCondition;
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_declarations::NamesFn;
use campfire_declarations::{
    Book, BuildFn, Cache, Callbacks, ColumnFill, Declarations, EntityKind, ExtendFn, Fact,
    KindSpawn, KindVisitor, Listed, ModuleSystems, OneShot, Report, Role, RuleFn, Scratch, Section,
};
use campfire_script::{
    ApiBuilder, CallExtension, CallParams, EventPhase, HookDecl, ModeEvents, PlannedCapability,
    RowReads, Status, ViewColumn,
};
use campfire_sim::{
    Breach, Capability, HostRule, OwnResources, Removed, SimComponent, SimResource, TimedProcess,
};
use campfire_states::{Replication, StateTypes};
use campfire_values::EngineTag;
use campfire_values::{MatchScope, NameId, Named, Names};

/// The consumer that clears a match's derived state for a load in place, its sim entities
/// despawned (08-sessions.md, A load goes back in place): each cache clears as it says, each
/// report goes back to its install's, and each removal the despawn recorded goes. A due index
/// builds again as the restore rewrites the state, a scratch is clear between ticks, and the
/// books, the facts and what the load put in are the packages', which a load keeps.
#[derive(Debug)]
pub(super) struct StateClear<'a> {
    pub(super) world: &'a mut World,
    pub(super) role: Role,
}

impl StateTypes for StateClear<'_> {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for StateClear<'_> {
    fn kind<K: EntityKind>(&mut self) {}

    fn state<K: EntityKind, S: Component>(&mut self) {}
}

impl Declarations for StateClear<'_> {
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

    fn section<S: Section>(&mut self) {}

    fn listed<L: Listed>(&mut self) {}

    fn sets<S: ModuleSystems>(&mut self) {}

    /// A scratch is clear between the ticks of a running match already, as its audit holds.
    fn scratch<R: Scratch>(&mut self) {
        debug_assert!(
            self.world.get_resource::<R>().is_none_or(Scratch::is_clear),
            "{} is clear between ticks",
            type_name::<R>()
        );
    }

    fn cache<R: Cache>(&mut self) {
        if let Some(mut cache) = self.world.get_resource_mut::<R>() {
            cache.clear();
        }
    }

    fn timed<P: TimedProcess>(&mut self) {}

    /// A report goes back to what the install put in, as the old match's record is of no tick
    /// the loaded one runs.
    fn report<R: Report>(&mut self) {
        if self.world.remove_resource::<R>().is_some()
            && let Some(report) = R::installed(&self.role)
        {
            self.world.insert_resource(report);
        }
    }

    fn mode_events<Q: ModeEvents>(&mut self, _: EventPhase) {}

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

    /// The removals the despawn of the old units recorded name no unit of the loaded match.
    fn removals<C: Component>(&mut self) {
        if let Some(mut removed) = self.world.get_resource_mut::<Messages<Removed<C>>>() {
            removed.clear();
        }
    }

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, R::Out, M> + 'static) {}

    fn loaded<R: Resource>(&mut self) {}

    fn stages_run_if<M>(&mut self, _: impl SystemCondition<M> + Clone) {}

    fn server_part<H: Component, C: Component + Default>(&mut self) {}

    fn part_with<H: Component, C: Component + Default>(&mut self) {}

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> Option<R>) {}
}

impl OwnResources for StateClear<'_> {
    fn own<R: Resource>(&mut self) {}
}
