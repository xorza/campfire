use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};

use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::parted_kind::PartedKind;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::entity_kind::EntityKind;
use crate::units::row_parts::RowParts;
use crate::units::view_column::ViewColumn;

/// A consumer of the kinds of entity alone, as the network layer reads them: it passes each kind
/// to `V`, and ignores every other declaration.
#[derive(Debug)]
pub(super) struct KindsOnly<'a, V>(pub(super) &'a mut V);

impl<V> StateTypes for KindsOnly<'_, V> {
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

impl<V: KindVisitor> KindVisitor for KindsOnly<'_, V> {
    fn kind<K: EntityKind>(&mut self) {
        self.0.kind::<K>();
    }
}

impl<V: KindVisitor> Declarations for KindsOnly<'_, V> {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {}

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {}

    fn column<C: ViewColumn + Default, D: RowParts>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<K: PartedKind, C: SimComponent>(&mut self, _: fn(&World) -> C) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
