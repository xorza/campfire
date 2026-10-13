use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};

use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_spawn::KindSpawn;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::module_set::ModuleSet;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::state_types::StateTypes;
use crate::state_types::replication::Replication;
use crate::units::engine_tag::EngineTag;
use crate::units::entity_kind::EntityKind;
use crate::units::row_reads::RowReads;
use crate::units::view_column::ViewColumn;

/// The consumer that gathers the engine tags each module declares, in the order the modules
/// declare them.
#[derive(Debug, Default)]
pub(super) struct EngineTagList(pub(super) Vec<EngineTag>);

impl StateTypes for EngineTagList {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for EngineTagList {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for EngineTagList {
    fn module(&mut self, _: Option<Capability>) {}

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

    fn engine_tags(&mut self, tags: &'static [EngineTag]) {
        self.0.extend_from_slice(tags);
    }

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
