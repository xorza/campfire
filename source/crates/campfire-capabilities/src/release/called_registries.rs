use std::any::TypeId;
use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};

use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::entity_kind::EntityKind;
use crate::units::row_reads::RowReads;
use crate::units::view_column::ViewColumn;

/// The consumer that gathers each registry of callbacks a module calls or declares a callback in,
/// so an install asks the declarations whether the match calls for it.
#[derive(Debug, Default)]
pub(super) struct CalledRegistries(pub(super) Vec<TypeId>);

impl StateTypes for CalledRegistries {
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

impl KindVisitor for CalledRegistries {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for CalledRegistries {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {
        self.0.push(TypeId::of::<C>());
    }

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {
        self.0.push(TypeId::of::<C>());
    }

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<K: EntityKind + SimComponent, C: SimComponent>(&mut self, _: fn(&World) -> C) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
