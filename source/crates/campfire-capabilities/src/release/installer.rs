use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource, StateRegistry};

use crate::declare::book::Book;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::parted_kind::PartedKind;
use crate::players::kind_parts::KindParts;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::scripts::ctx::Ctx;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::entity_kind::EntityKind;
use crate::units::row_parts::RowParts;
use crate::units::view::View;
use crate::units::view_column::ViewColumn;

/// The consumer that puts a module's declarations into a match: its state types and restore
/// checks into the state registry, and the rest into the world.
#[derive(Debug)]
pub(super) struct Installer<'a> {
    world: &'a mut World,
    registry: &'a mut StateRegistry,
    set: CapabilitySet,
    scripts: bool,
}

impl<'a> Installer<'a> {
    /// Installs into `world` and `registry`, of a match of the capabilities `set` that runs
    /// scripts when `scripts` says.
    pub(super) const fn new(
        world: &'a mut World,
        registry: &'a mut StateRegistry,
        set: CapabilitySet,
        scripts: bool,
    ) -> Installer<'a> {
        Installer {
            world,
            registry,
            set,
            scripts,
        }
    }
}

impl StateTypes for Installer<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, declared: Declared) {
        self.registry.once::<C>(declared);
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(
        &mut self,
        declared: Declared,
    ) {
        self.registry.on_insert::<C>(declared);
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, declared: Declared) {
        self.registry.on_change::<C>(declared);
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        declared: Declared,
    ) {
        self.registry.predicted::<C>(declared);
    }

    fn server<C: Replication>(&mut self, declared: Declared) {
        self.registry.server::<C>(declared);
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource>(&mut self) {
        self.registry.resource::<R>();
    }

    fn sim_component<C: Replication>(&mut self) {
        self.registry.sim_component::<C>();
    }
}

impl KindVisitor for Installer<'_> {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for Installer<'_> {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {
        self.registry.require::<H, C>();
    }

    fn host<C: SimComponent, H: HostRule>(&mut self) {
        self.registry.host::<C, H>();
    }

    fn check<C: SimComponent>(&mut self, check: fn(&World) -> Result<(), Breach>) {
        self.registry.add_check::<C>(check);
    }

    fn book<B: Book>(&mut self) {
        if B::held(self.set, self.scripts) {
            self.world.insert_resource(B::default());
        }
    }

    fn column<C: ViewColumn + Default, D: RowParts>(&mut self, fill: ColumnFill<D, C>) {
        let view = self.world.non_send::<View>().clone();
        view.add_column(C::default());
        view.add_source::<D, C>(self.world, fill);
    }

    fn call_part<P: CallPart + Default>(&mut self) {
        if self.scripts {
            self.world.non_send::<Ctx>().frame().add_part(P::default());
        }
    }

    fn part<K: PartedKind, C: SimComponent>(&mut self, start: fn(&World) -> C) {
        self.world
            .resource_mut::<KindParts<K>>()
            .add(move |entity| {
                let part = start(entity.world());
                entity.insert(part);
            });
        self.registry.require::<K, C>();
    }

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
