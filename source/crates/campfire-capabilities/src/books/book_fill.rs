use std::any::type_name;
use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};

use crate::books::Books;
use crate::books::built_books::BuiltBooks;
use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::engine_tag::EngineTag;
use crate::units::entity_kind::EntityKind;
use crate::units::row_reads::RowReads;
use crate::units::view_column::ViewColumn;

/// The install of the books, a walk of every module's declarations: each declared book is taken
/// from `built`, which must hold it, and goes in `world`, a match of `set` that runs scripts when
/// `scripts` says, when the match holds it; one the match does not hold is dropped.
#[derive(Debug)]
pub(super) struct BookFill<'a> {
    pub(super) built: &'a mut BuiltBooks,
    pub(super) world: &'a mut World,
    pub(super) set: CapabilitySet,
    pub(super) scripts: bool,
}

impl StateTypes for BookFill<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {}

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {}

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {}

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
    }

    fn server<C: Replication>(&mut self, _: Declared) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for BookFill<'_> {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for BookFill<'_> {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {
        let book = self.built.take::<B>();
        let book = book.unwrap_or_else(|| panic!("{} is declared and built", type_name::<B>()));
        if B::held(self.set, self.scripts) {
            Books::load(self.world, book);
        }
    }

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {}

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {}

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<K: EntityKind + SimComponent, C: SimComponent>(&mut self, _: fn(&World) -> C) {}

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
