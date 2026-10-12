use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};

use crate::declare::book::Book;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::parted_kind::PartedKind;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::entity_kind::EntityKind;
use crate::units::row_parts::RowParts;
use crate::units::view_column::ViewColumn;

/// The consumer that binds each module's script API, of the module's capability: it reads the
/// modules and their APIs, and ignores every other declaration.
#[derive(Debug)]
pub(super) struct ApiBinder<'b, 'a>(pub(super) &'b mut ApiBuilder<'a>);

impl StateTypes for ApiBinder<'_, '_> {
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

    fn resource<R: SimResource>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for ApiBinder<'_, '_> {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for ApiBinder<'_, '_> {
    fn module(&mut self, module: Option<Capability>) {
        self.0.module(module);
    }

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {}

    fn column<C: ViewColumn + Default, D: RowParts>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<K: PartedKind, C: SimComponent>(&mut self, _: fn(&World) -> C) {}

    fn api(&mut self, register: fn(&mut ApiBuilder<'_>)) {
        register(self.0);
    }
}
