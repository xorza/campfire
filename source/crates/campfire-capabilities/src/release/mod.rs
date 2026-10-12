use std::num::NonZeroU64;

use campfire_script::ScriptHost;
use campfire_sim::{Capability, StateRegistry};

use crate::abilities::Abilities;
use crate::actions::Actions;
use crate::areas::Areas;
use crate::combat::Combat;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::declare::kind_visitor::KindVisitor;
use crate::deliveries::Deliveries;
use crate::items::Items;
use crate::mode::Mode;
use crate::navigation::Navigation;
use crate::orders::Orders;
use crate::production::Production;
use crate::progression::Progression;
use crate::projectiles::Projectiles;
use crate::release::api_binder::ApiBinder;
use crate::release::installer::Installer;
use crate::release::state_only::StateOnly;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::ScriptApi;
use crate::state_types::StateTypes;
use crate::stats::Stats;
use crate::units::Units;
use crate::vision::Vision;

mod api_binder;
mod installer;
pub(crate) mod r#match;
pub(crate) mod match_spec;
pub(crate) mod release_world;
mod state_only;

/// The engine's modules as the release runs them: the core, each capability in the order they
/// install, and the mode, which installs itself once the others did.
#[derive(Debug)]
pub struct Release;

/// A capability's install into a match, after its declarations.
type Install = fn(&mut InstallCx<'_>);

/// One row of `CAPABILITIES`: the capability, and how it installs, `None` for one the release
/// does not run yet and for the mode.
#[derive(Debug, Clone, Copy)]
struct Row {
    capability: Capability,
    install: Option<Install>,
}

const fn row(capability: Capability, install: Install) -> Row {
    Row {
        capability,
        install: Some(install),
    }
}

const fn planned(capability: Capability) -> Row {
    Row {
        capability,
        install: None,
    }
}

/// Every capability once, in the order they install: each after the ones it builds on, and orders
/// after production and items, whose actions it applies when they are installed. A declared
/// capability the release does not run yet installs nothing.
const CAPABILITIES: [Row; Capability::ALL.len()] = [
    row(Capability::Stats, Stats::install),
    row(Capability::Progression, Progression::install),
    row(Capability::Combat, Combat::install),
    row(Capability::Navigation, Navigation::install),
    row(Capability::Vision, Vision::install),
    row(Capability::Projectiles, Projectiles::install),
    row(Capability::Areas, Areas::install),
    row(Capability::Abilities, Abilities::install),
    row(Capability::Production, Production::install),
    row(Capability::Items, Items::install),
    row(Capability::Orders, Orders::install),
    planned(Capability::Character),
    planned(Capability::Hitboxes),
    planned(Capability::Physics),
    planned(Capability::World),
    planned(Capability::Quests),
    planned(Capability::Interaction),
    planned(Capability::Mode),
];

impl Release {
    /// Every capability the release runs.
    pub fn capabilities() -> CapabilitySet {
        let runs: Vec<Capability> = CAPABILITIES
            .iter()
            .filter(|row| row.install.is_some())
            .map(|row| row.capability)
            .collect();
        CapabilitySet::new(&runs).expect("the release runs each capability with its needs")
    }

    /// The script API of the release, recorded as a match's engine binds it, with the names the
    /// engine has before: every capability's, whether a mode declares it or not.
    pub fn script_api() -> ScriptApi {
        Release::bind_script_api(&mut ScriptHost::new(NonZeroU64::MIN))
    }

    /// Binds the script API of the release into `host`, as `script_api` records it, so `host`
    /// compiles a script as a match's engine does: with the engine enums' modules.
    pub fn bind_script_api(host: &mut ScriptHost) -> ScriptApi {
        ScriptApi::release(host, Release::bind_api)
    }

    /// Visits, with `d`, the declarations of a match of `set`: the core's, then each declared
    /// capability's in the order they install, then the mode's.
    pub(crate) fn declare<D: Declarations>(set: CapabilitySet, d: &mut D) {
        Release::core(d);
        for row in CAPABILITIES {
            if set.contains(row.capability) {
                Release::declare_capability(row.capability, d);
            }
        }
        Release::module::<Mode, D>(d);
    }

    /// Lists, into `types`, the state types of a match of `set`, as its install registers them.
    pub fn state_types<T: StateTypes>(set: CapabilitySet, types: &mut T) {
        Release::declare(set, &mut StateOnly(types));
    }

    /// Visits, with `v`, each kind of entity of a match, whatever its capabilities.
    pub fn kinds<V: KindVisitor>(v: &mut V) {
        Units::kinds(v);
    }

    /// Binds into `builder` the script API of every module the release runs.
    fn bind_api(builder: &mut ApiBuilder<'_>) {
        Release::declare(Release::capabilities(), &mut ApiBinder(builder));
    }

    /// Installs the core's module `M`: its declarations, then `install`.
    fn install_module<M: Declares>(
        cx: &mut InstallCx<'_>,
        registry: &mut StateRegistry,
        install: Install,
    ) {
        let (set, scripts) = (cx.set(), cx.scripts());
        Release::module::<M, _>(&mut Installer::new(cx.world, registry, set, scripts));
        install(cx);
    }

    /// Visits the core's declarations.
    fn core<D: Declarations>(d: &mut D) {
        Release::module::<Units, D>(d);
        Release::module::<Actions, D>(d);
        Release::module::<Deliveries, D>(d);
    }

    /// Visits the declarations of `capability`; none for one the release does not run yet. The
    /// match is exhaustive, so a capability with no arm does not compile.
    fn declare_capability<D: Declarations>(capability: Capability, d: &mut D) {
        match capability {
            Capability::Stats => Release::module::<Stats, D>(d),
            Capability::Progression => Release::module::<Progression, D>(d),
            Capability::Combat => Release::module::<Combat, D>(d),
            Capability::Navigation => Release::module::<Navigation, D>(d),
            Capability::Vision => Release::module::<Vision, D>(d),
            Capability::Projectiles => Release::module::<Projectiles, D>(d),
            Capability::Areas => Release::module::<Areas, D>(d),
            Capability::Abilities => Release::module::<Abilities, D>(d),
            Capability::Production => Release::module::<Production, D>(d),
            Capability::Items => Release::module::<Items, D>(d),
            Capability::Orders => Release::module::<Orders, D>(d),
            Capability::Character
            | Capability::Hitboxes
            | Capability::Physics
            | Capability::World
            | Capability::Quests
            | Capability::Interaction
            | Capability::Mode => {}
        }
    }

    /// Visits the declarations of `M`, after the mark of its start.
    fn module<M: Declares, D: Declarations>(d: &mut D) {
        d.module(M::MODULE);
        M::declare(d);
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::any::TypeId;
    use std::collections::BTreeSet;
    use std::fmt::Debug;

    use bevy_ecs::archetype::Archetype;
    use bevy_ecs::component::{Component, Immutable, Mutable};
    use bevy_ecs::world::World;
    use campfire_sim::{SimResource, StableId};

    use crate::declare::capability_set::CapabilitySet;
    use crate::release::Release;
    use crate::state_types::replication::Replication;
    use crate::state_types::{Declared, StateTypes};

    /// The component types the lists declare, as state or as derived, by their ids.
    #[derive(Debug, Default)]
    struct ListedTypes(BTreeSet<TypeId>);

    impl ListedTypes {
        fn push<C: Component>(&mut self) {
            self.0.insert(TypeId::of::<C>());
        }
    }

    impl StateTypes for ListedTypes {
        fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn predicted<
            C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug,
        >(
            &mut self,
            _: Declared,
        ) {
            self.push::<C>();
        }

        fn server<C: Replication>(&mut self, _: Declared) {
            self.push::<C>();
        }

        fn derived<C: Component>(&mut self) {
            self.push::<C>();
        }

        fn resource<R: SimResource>(&mut self) {}

        fn sim_component<C: Replication>(&mut self) {
            self.push::<C>();
        }
    }

    impl Release {
        /// The names of the component types a unit of `world`, an entity with a stable id, holds
        /// or held, that no list of the capabilities `set` declares as state or as derived, sorted:
        /// each a part of a unit that no hash, snapshot or client holds. The stable id is the
        /// entity list's, which the state registry holds.
        pub fn unlisted_components(set: CapabilitySet, world: &World) -> Vec<String> {
            let mut listed = ListedTypes::default();
            Release::state_types(set, &mut listed);
            listed.push::<StableId>();
            let components = world.components();
            let Some(unit) = components.component_id::<StableId>() else {
                return Vec::new();
            };
            let mut names: Vec<String> = world
                .archetypes()
                .iter()
                .filter(|archetype| archetype.contains(unit))
                .flat_map(Archetype::components)
                .filter_map(|&id| components.get_info(id))
                .filter(|info| info.type_id().is_none_or(|id| !listed.0.contains(&id)))
                .map(|info| info.name().to_string())
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
    }
}

#[cfg(test)]
mod tests;
