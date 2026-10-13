use std::any::{TypeId, type_name};
use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use bevy_ecs::world::World;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource, SimWorldBuilder};

use crate::declare::book::Book;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::entity_parts::EntityParts;
use crate::declare::kind_spawn::KindSpawn;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::module_set::ModuleSet;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::mode::mode_world::ModeWorld;
use crate::release::Release;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};
use crate::units::engine_tag::EngineTag;
use crate::units::entity_kind::EntityKind;
use crate::units::entity_kinds::EntityKinds;
use crate::units::predicting::Predicting;
use crate::units::row_reads::RowReads;
use crate::units::view_column::ViewColumn;
use crate::visibility::OutView;

/// A scratch resource, by its name, and whether a world holds it clear.
#[derive(Debug, Clone, Copy)]
struct ScratchCheck {
    name: &'static str,
    clear: fn(&World) -> bool,
}

/// The consumer that gathers every resource the modules declare, of every kind, and the parts of
/// each kind the installer sets up, so the resource audit finds a resource none names.
#[derive(Debug, Default)]
pub(super) struct DeclaredResources {
    ids: Vec<TypeId>,
    scratch: Vec<ScratchCheck>,
}

impl DeclaredResources {
    /// The resources of `world`, a match of `set`, that no declaration names, the release's
    /// own, the sim's own and those of `host` aside, then, while the match runs, each scratch
    /// resource not clear, each by its name.
    pub(super) fn audit(set: CapabilitySet, world: &World, host: &[TypeId]) -> Vec<String> {
        let mut declared = DeclaredResources::default();
        Release::declare(set, &mut declared);
        declared.ids.extend(DeclaredResources::release_own());
        declared.ids.extend(SimWorldBuilder::sim_resources());
        declared.ids.extend(host);
        let mut unlisted: Vec<String> = world
            .iter_resources()
            .filter(|(info, _)| info.type_id().is_none_or(|id| !declared.ids.contains(&id)))
            .map(|(info, _)| info.name().to_string())
            .collect();
        unlisted.sort_unstable();
        if world.match_end().is_some() {
            return unlisted;
        }
        unlisted.extend(
            declared
                .scratch
                .iter()
                .filter(|check| !(check.clear)(world))
                .map(|check| format!("{} is not clear", check.name)),
        );
        unlisted
    }

    /// What the release puts in a match itself: its capabilities, what a client's view of it
    /// leaves out, its script budgets and the mark of a client's.
    const fn release_own() -> [TypeId; 4] {
        [
            TypeId::of::<CapabilitySet>(),
            TypeId::of::<OutView>(),
            TypeId::of::<ScriptBudgets>(),
            TypeId::of::<Predicting>(),
        ]
    }
}

impl StateTypes for DeclaredResources {
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

    fn resource<R: SimResource + Default>(&mut self) {
        self.ids.push(TypeId::of::<R>());
    }

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for DeclaredResources {
    fn kind<K: EntityKind>(&mut self) {
        self.ids.push(TypeId::of::<EntityKinds>());
    }
}

impl Declarations for DeclaredResources {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {
        self.ids.push(TypeId::of::<B>());
    }

    fn sets<S: ModuleSet>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {
        self.ids.push(TypeId::of::<R>());
        self.scratch.push(ScratchCheck {
            name: type_name::<R>(),
            clear: |world| world.get_resource::<R>().is_none_or(R::is_clear),
        });
    }

    fn cache<R: Cache>(&mut self) {
        self.ids.push(TypeId::of::<R>());
    }

    fn report<R: Report>(&mut self) {
        self.ids.push(TypeId::of::<R>());
    }

    fn calls<C: Callbacks>(&mut self) {
        self.ids.push(TypeId::of::<C>());
    }

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {
        self.ids.push(TypeId::of::<C>());
    }

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
        self.ids.push(TypeId::of::<EntityParts<H>>());
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}
