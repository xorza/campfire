use bevy_ecs::bundle::Bundle;
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use std::any::type_name;
use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::world::World;
use campfire_declare::{
    Book, Cache, Callbacks, ColumnFill, Declarations, EntityKind, KindSpawn, KindVisitor,
    ModuleSet, OneShot, Report, Role, Scratch,
};
use campfire_script::{ApiBuilder, CallParams, CallPart, RowReads, ViewColumn};
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};
use campfire_state_types::{Replication, StateTypes};
use campfire_values::EngineTag;

use crate::books::built_books::BuiltBooks;

/// The install of the books, a walk of every module's declarations: each declared book is taken
/// from `built`, which must hold it, and goes in `world` in place of the empty one its owner's
/// declaration put there; one the match does not hold, as its install put none, is dropped.
#[derive(Debug)]
pub(super) struct BookFill<'a> {
    pub(super) built: &'a mut BuiltBooks,
    pub(super) world: &'a mut World,
}

impl StateTypes for BookFill<'_> {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for BookFill<'_> {
    fn kind<K: EntityKind>(&mut self) {}

    fn state<K: EntityKind, S: Component>(&mut self) {}
}

impl Declarations for BookFill<'_> {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {
        let book = self.built.take::<B>();
        let book = book.unwrap_or_else(|| panic!("{} is declared and built", type_name::<B>()));
        if self.world.contains_resource::<B>() {
            book.load_into(self.world);
        }
    }

    fn sets<S: ModuleSet>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {}

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {}

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn call_params<P: CallParams + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}

    fn observer<E: Event, B: Bundle, M>(&mut self, _: impl IntoObserverSystem<E, B, M>) {}

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, (), M> + 'static) {}

    fn server_part<H: Component, C: Component + Default>(&mut self) {}

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> R) {}
}
