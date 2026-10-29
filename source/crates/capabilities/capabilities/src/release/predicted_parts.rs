use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::EventPattern;
use bevy_ecs::lifecycle::{Add, ComponentHook};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, Has};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::SystemCondition;
use bevy_ecs::system::{Commands, IntoObserverSystem, IntoSystem, Query};
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
    Breach, Capability, HostRule, OwnResources, SimComponent, SimResource, TimedProcess,
    Unpredicted,
};
use campfire_states::{Replication, StateTypes};
use campfire_values::EngineTag;
use campfire_values::{MatchScope, NameId, Named, Names};

use crate::release::client_holds::ClientHolds;

/// The walk of a match's lists that gives a predicting test client what replication brings the
/// units it predicts of the server's own parts, as their owner's client receives them, which a
/// client's spawn does not give (02-engine-core.md, The test client holds what a
/// networked client holds).
#[derive(Debug)]
pub(super) struct PredictedParts<'a> {
    pub(super) world: &'a mut World,
    pub(super) holds: &'a ClientHolds,
}

impl StateTypes for PredictedParts<'_> {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for PredictedParts<'_> {
    fn kind<K: EntityKind>(&mut self) {}

    fn state<K: EntityKind, S: Component>(&mut self) {}
}

impl Declarations for PredictedParts<'_> {
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

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn timed<P: TimedProcess>(&mut self) {}

    fn report<R: Report>(&mut self) {}

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

    fn removals<C: Component>(&mut self) {}

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, R::Out, M> + 'static) {}

    fn loaded<R: Resource>(&mut self) {}

    fn stages_run_if<M>(&mut self, _: impl SystemCondition<M> + Clone) {}

    fn part_with<H: Component, C: Component + Default>(&mut self) {}

    /// Registers, for a part a predicted unit's client receives, the insert its replication
    /// brings: the part's default on each unit of the host `H` that its client predicts.
    fn server_part<H: Component, C: Component + Default>(&mut self) {
        let id = self.world.register_component::<C>();
        let received = self
            .holds
            .get(id)
            .is_some_and(|(_, held)| held.reaches(true));
        if received {
            self.world.add_observer(
                |added: On<'_, '_, Add<H>>,
                 mut commands: Commands<'_, '_>,
                 units: Query<'_, '_, (Has<Unpredicted>, Has<C>), Allow<Unpredicted>>| {
                    if units.get(added.entity) == Ok((false, false)) {
                        commands.entity(added.entity).insert(C::default());
                    }
                },
            );
        }
    }

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> Option<R>) {}
}

impl OwnResources for PredictedParts<'_> {
    fn own<R: Resource>(&mut self) {}
}
