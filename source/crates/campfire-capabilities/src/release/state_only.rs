use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Capability, HostRule, SimComponent, SimResource};

use crate::declare::book::Book;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::row_parts::RowParts;
use crate::units::view_column::ViewColumn;

/// A consumer of the state types alone, as the network layer reads them: it passes each state
/// type to `T`, and ignores every other declaration.
#[derive(Debug)]
pub(super) struct StateOnly<'a, T>(pub(super) &'a mut T);

impl<T: StateTypes> StateTypes for StateOnly<'_, T> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, declared: Declared) {
        self.0.once::<C>(declared);
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, declared: Declared) {
        self.0.on_change::<C>(declared);
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        declared: Declared,
    ) {
        self.0.predicted::<C>(declared);
    }

    fn server<C: Replication>(&mut self, declared: Declared) {
        self.0.server::<C>(declared);
    }

    fn derived<C: Component>(&mut self) {
        self.0.derived::<C>();
    }

    fn resource<R: SimResource>(&mut self) {
        self.0.resource::<R>();
    }

    fn sim_component<C: Replication>(&mut self) {
        self.0.sim_component::<C>();
    }
}

impl<T: StateTypes> Declarations for StateOnly<'_, T> {
    fn module(&mut self, _: Option<Capability>) {}

    fn kind<K: HostRule + Component>(&mut self) {}

    fn require<H: Component, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> bool) {}

    fn book<B: Book>(&mut self) {}

    fn column<C: ViewColumn + Default, D: RowParts>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn player_part<C: SimComponent>(&mut self, _: fn(&World) -> C) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
