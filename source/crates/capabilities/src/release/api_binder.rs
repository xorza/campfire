use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::world::World;
use campfire_core::{
    ApiBuilder, Book, Cache, CallPart, Callbacks, ColumnFill, Declarations, EngineTag, EntityKind,
    KindSpawn, KindVisitor, ModuleSet, Report, RowReads, Scratch, ViewColumn,
};
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};
use campfire_state_types::{Replication, StateTypes};

/// The consumer that binds each module's script API, of the module's capability: it reads the
/// modules and their APIs, and ignores every other declaration.
#[derive(Debug)]
pub(super) struct ApiBinder<'b, 'a>(pub(super) &'b mut ApiBuilder<'a>);

impl StateTypes for ApiBinder<'_, '_> {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for ApiBinder<'_, '_> {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for ApiBinder<'_, '_> {
    fn module(&mut self, capability: Option<Capability>) {
        self.0.module(capability);
    }

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {}

    fn sets<S: ModuleSet>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {}

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {}

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, register: fn(&mut ApiBuilder<'_>)) {
        register(self.0);
    }
}
