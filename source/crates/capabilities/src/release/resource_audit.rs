use std::any::{TypeId, type_name};
use std::fmt::Debug;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::ComponentHook;
use bevy_ecs::message::Messages;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::SystemCondition;
use bevy_ecs::system::{IntoObserverSystem, IntoSystem};
use bevy_ecs::world::World;
use campfire_declare::{
    Book, BuildFn, Cache, Callbacks, CapabilitySet, ColumnFill, Declarations, EntityKind,
    EntityParts, ExtendFn, Fact, KindSpawn, KindVisitor, Listed, ModuleSet, OneShot, Report, Role,
    RuleFn, Scratch, Section, SpawnParts,
};
use campfire_mode::ModeWorld;
use campfire_script::{ApiBuilder, CallParams, CallPart, RowReads, ViewColumn};
use campfire_sim::{
    Breach, Capability, HostRule, OwnResources, Removed, SimComponent, SimResource,
};
use campfire_state_types::{Replication, StateTypes};
use campfire_units::EntityKinds;
use campfire_values::EngineTag;

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
    /// The resources of `world`, a match of `set`, that no declaration names, those of `host`
    /// aside, then, while the match runs, each scratch
    /// resource not clear, each by its name.
    pub(super) fn audit(set: CapabilitySet, world: &World, host: &[TypeId]) -> Vec<String> {
        let mut declared = Self::default();
        Release::declare(set, &mut declared);
        declared.ids.extend(host);
        // Bevy's walk of the resources leaves the non-send ones out, which its storage lists.
        let components = world.components();
        let non_sends = world
            .storages()
            .non_sends
            .iter()
            .filter(|(_, data)| data.is_present())
            .filter_map(|(id, _)| components.get_info(id));
        let mut unlisted: Vec<String> = world
            .iter_resources()
            .map(|(info, _)| info)
            .chain(non_sends)
            .filter(|info| info.type_id().is_none_or(|id| !declared.ids.contains(&id)))
            .map(|info| info.name().to_string())
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

    fn fact<F: Fact>(&mut self, _: BuildFn<F>) {}

    fn extend<B: Send + Sync + 'static>(&mut self, _: ExtendFn<B>) {}

    fn rule(&mut self, _: RuleFn) {}

    fn book<B: Book>(&mut self, _: BuildFn<B>) {
        self.ids.push(TypeId::of::<B>());
    }

    fn section<S: Section>(&mut self) {}

    fn listed<L: Listed>(&mut self) {}

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

    fn removals<C: Component>(&mut self) {
        self.ids.push(TypeId::of::<Messages<Removed<C>>>());
    }

    fn one_shot<R: OneShot, M>(&mut self, _: impl IntoSystem<R::In, (), M> + 'static) {
        self.ids.push(TypeId::of::<R>());
    }

    fn server_part<H: Component, C: Component + Default>(&mut self) {}

    fn on_insert<C: Component>(&mut self, _: ComponentHook) {}

    fn non_send<R: 'static>(&mut self, _: fn(&World, &Role) -> Option<R>) {
        self.ids.push(TypeId::of::<R>());
    }

    fn loaded<R: Resource>(&mut self) {
        self.ids.push(TypeId::of::<R>());
    }

    fn stages_run_if<M>(&mut self, _: impl SystemCondition<M> + Clone) {}
}

impl OwnResources for DeclaredResources {
    fn own<R: Resource>(&mut self) {
        self.ids.push(TypeId::of::<R>());
    }
}
