use bevy_ecs::bundle::Bundle;
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use std::any::{TypeId, type_name};
use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::world::World;
use campfire_declare::{
    Book, Cache, Callbacks, CapabilitySet, ColumnFill, Declarations, EntityKind, EntityParts,
    KindSpawn, KindVisitor, ModuleSet, OneShot, Report, Role, Scratch, SpawnParts,
};
use campfire_mode::ModeWorld;
use campfire_script::{ApiBuilder, CallParams, CallPart, RowReads, ScriptBudgets, ViewColumn};
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource, SimWorldBuilder};
use campfire_state_types::{Replication, StateTypes};
use campfire_units::EntityKinds;
use campfire_values::EngineTag;
use campfire_visibility::OutView;

use crate::release::Release;

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
        let mut declared = Self::default();
        Release::declare(set, &mut declared);
        declared.ids.extend(Self::release_own());
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
    /// leaves out, and its script budgets.
    const fn release_own() -> [TypeId; 3] {
        [
            TypeId::of::<CapabilitySet>(),
            TypeId::of::<OutView>(),
            TypeId::of::<ScriptBudgets>(),
        ]
    }
}

impl StateTypes for DeclaredResources {
    fn component<C: Replication>(&mut self) {}

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

    fn state<K: EntityKind, S: Component>(&mut self) {}
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

    fn call_params<P: CallParams + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
        self.ids.push(TypeId::of::<EntityParts<H>>());
        self.ids.push(TypeId::of::<SpawnParts>());
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}

    fn observer<E: Event, B: Bundle, M>(&mut self, _: impl IntoObserverSystem<E, B, M>) {}

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, (), M> + 'static) {
        self.ids.push(TypeId::of::<R>());
    }

    fn server_part<H: Component, C: Component + Default>(&mut self) {}

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> R) {
        self.ids.push(TypeId::of::<R>());
    }
}
