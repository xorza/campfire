use std::num::NonZeroU64;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_script::ScriptHost;
use campfire_sim::{Capability, StateRegistry};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::abilities::Abilities;
use crate::abilities::abilities_api::AbilitiesApi;
use crate::actions::Actions;
use crate::actions::actions_api::ActionsApi;
use crate::areas::Areas;
use crate::areas::areas_api::AreasApi;
use crate::capability_set::error::CapabilityError;
use crate::combat::Combat;
use crate::combat::combat_api::CombatApi;
use crate::deliveries::deliveries_api::DeliveriesApi;
use crate::items::Items;
use crate::items::items_api::ItemsApi;
use crate::mode::Mode;
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_api::ModeApi;
use crate::navigation::Navigation;
use crate::navigation::navigation_api::NavigationApi;
use crate::orders::Orders;
use crate::orders::orders_api::OrdersApi;
use crate::production::Production;
use crate::production::production_api::ProductionApi;
use crate::progression::Progression;
use crate::progression::progression_api::ProgressionApi;
use crate::projectiles::Projectiles;
use crate::projectiles::projectiles_api::ProjectilesApi;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::state_types::StateTypes;
use crate::stats::Stats;
use crate::stats::stats_api::StatsApi;
use crate::units::Units;
use crate::vision::Vision;
use crate::vision::vision_api::VisionApi;

pub(crate) mod error;

/// A mode's declared capabilities: none twice, not `mode`, which every match has, and each with
/// the ones it builds on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySet(u32);

/// A capability's install into a match.
type Install = fn(&mut World, &mut Schedule, &mut StateRegistry);
type RegisterApi = fn(&mut ApiBuilder<'_>);

/// One row of `CAPABILITIES`: the capability, how it installs, `None` for one the release does
/// not run yet and for the mode, which installs itself once the others did; the capabilities it
/// builds on; and how it registers its script API, when it has one.
#[derive(Debug, Clone, Copy)]
struct Row {
    capability: Capability,
    install: Option<Install>,
    needs: &'static [Capability],
    api: Option<RegisterApi>,
}

const fn row(capability: Capability, install: Install, needs: &'static [Capability]) -> Row {
    Row {
        capability,
        install: Some(install),
        needs,
        api: None,
    }
}

const fn planned(capability: Capability) -> Row {
    Row {
        capability,
        install: None,
        needs: &[],
        api: None,
    }
}

impl Row {
    /// The same row, whose capability registers its script API by `register`.
    const fn registering(self, register: RegisterApi) -> Row {
        Row {
            api: Some(register),
            ..self
        }
    }
}

/// Every capability once, in the order they install: each after the ones it builds on, and orders
/// after production and items, whose actions it applies when they are installed. A declared
/// capability the release does not run yet installs nothing.
const CAPABILITIES: [Row; Capability::ALL.len()] = [
    row(Capability::Stats, Stats::install, &[]).registering(StatsApi::register),
    row(
        Capability::Progression,
        Progression::install,
        &[Capability::Stats],
    )
    .registering(ProgressionApi::register),
    row(Capability::Combat, Combat::install, &[Capability::Stats]).registering(CombatApi::register),
    row(Capability::Navigation, Navigation::install, &[]).registering(NavigationApi::register),
    row(Capability::Vision, Vision::install, &[Capability::Combat])
        .registering(VisionApi::register),
    row(
        Capability::Projectiles,
        Projectiles::install,
        &[Capability::Combat],
    )
    .registering(ProjectilesApi::register),
    row(Capability::Areas, Areas::install, &[Capability::Combat]).registering(AreasApi::register),
    row(
        Capability::Abilities,
        Abilities::install,
        &[Capability::Combat],
    )
    .registering(AbilitiesApi::register),
    row(Capability::Production, Production::install, &[]).registering(ProductionApi::register),
    row(Capability::Items, Items::install, &[Capability::Stats]).registering(ItemsApi::register),
    row(
        Capability::Orders,
        Orders::install,
        &[Capability::Combat, Capability::Navigation],
    )
    .registering(OrdersApi::register),
    planned(Capability::Character),
    planned(Capability::Hitboxes),
    planned(Capability::Physics),
    planned(Capability::World),
    planned(Capability::Quests),
    planned(Capability::Interaction),
    planned(Capability::Mode).registering(ModeApi::register),
];

const _: () = assert!(
    Capability::ALL.len() <= u32::BITS as usize,
    "a set holds each capability in a bit of its u32"
);

impl CapabilitySet {
    /// The script API of the release, recorded as a match's engine binds it, with the names the
    /// engine has before: every capability's, whether a mode declares it or not.
    pub fn script_api() -> ScriptApi {
        CapabilitySet::bind_script_api(&mut ScriptHost::new(NonZeroU64::MIN))
    }

    /// Binds the script API of the release into `host`, as `script_api` records it, so `host`
    /// compiles a script as a match's engine does: with the engine enums' modules.
    pub fn bind_script_api(host: &mut ScriptHost) -> ScriptApi {
        ScriptApi::release(host, CapabilitySet::apis())
    }

    /// How the action pipeline and the deliveries, then each capability, register their script
    /// API, the capabilities in the table's order.
    pub(crate) fn apis() -> impl Iterator<Item = RegisterApi> {
        let capabilities = CAPABILITIES.into_iter().filter_map(|row| row.api);
        let pipeline: [RegisterApi; 2] = [ActionsApi::register, DeliveriesApi::register];
        pipeline.into_iter().chain(capabilities)
    }

    /// The set of `declared`; an error when one is `mode`, one is declared twice, or one lacks a
    /// capability it builds on.
    pub fn new(declared: &[Capability]) -> Result<CapabilitySet, CapabilityError> {
        let mut set = CapabilitySet(0);
        for &capability in declared {
            if capability == Capability::Mode {
                return Err(CapabilityError::DeclaresMode);
            }
            if set.contains(capability) {
                return Err(CapabilityError::Repeated(capability));
            }
            set.0 |= bit(capability);
        }
        for capability in set.iter() {
            if let Some(&needs) = needs(capability)
                .iter()
                .find(|&&needs| !set.contains(needs))
            {
                return Err(CapabilityError::Needs { capability, needs });
            }
        }
        Ok(set)
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & bit(capability) != 0
    }

    /// The declared capabilities, in the order of their indices.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |&capability| self.contains(capability))
    }

    /// Installs the core, then each declared capability the release runs, each after the ones it
    /// builds on. A match that ended runs no stage. With no script `budgets`, as on a client,
    /// which runs no scripts, the core has no script host, and each capability leaves out what
    /// runs scripts.
    pub fn install(
        self,
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        budgets: Option<ScriptBudgets>,
    ) {
        Units::install(world, schedule, registry, budgets);
        Actions::install(world, registry);
        if let Some(mut host) = world.get_non_send_mut::<ScriptHost>() {
            ScriptApi::bind(&mut host, CapabilitySet::apis());
        }
        MatchEnd::stop_stages(schedule);
        for row in CAPABILITIES {
            if let Some(install) = row.install
                && self.contains(row.capability)
            {
                install(world, schedule, registry);
            }
        }
    }

    /// Lists, into `types`, the state types of a match of these capabilities: the core's, the
    /// mode's and each declared capability's, as each one's `install` registers them. The match is
    /// exhaustive, so a capability with no list does not compile.
    pub fn state_types<T: StateTypes>(self, types: &mut T) {
        Units::state_types(types);
        Actions::state_types(types);
        Mode::state_types(types);
        for capability in self.iter() {
            match capability {
                Capability::Stats => Stats::state_types(types),
                Capability::Progression => Progression::state_types(types),
                Capability::Combat => Combat::state_types(types),
                Capability::Navigation => Navigation::state_types(types),
                Capability::Vision => Vision::state_types(types),
                Capability::Projectiles => Projectiles::state_types(types),
                Capability::Areas => Areas::state_types(types),
                Capability::Production => Production::state_types(types),
                Capability::Items => Items::state_types(types),
                Capability::Orders => Orders::state_types(types),
                Capability::Abilities
                | Capability::Character
                | Capability::Hitboxes
                | Capability::Physics
                | Capability::World
                | Capability::Quests
                | Capability::Interaction
                | Capability::Mode => {}
            }
        }
    }
}

/// A manifest's `capabilities`, checked as `CapabilitySet::new` checks them.
impl<'de> Deserialize<'de> for CapabilitySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CapabilitySet, D::Error> {
        let declared = Vec::<Capability>::deserialize(deserializer)?;
        CapabilitySet::new(&declared).map_err(D::Error::custom)
    }
}

const fn bit(capability: Capability) -> u32 {
    1 << capability as u32
}

/// The capabilities `capability` builds on.
const fn needs(capability: Capability) -> &'static [Capability] {
    let mut at = 0;
    while at < CAPABILITIES.len() {
        if CAPABILITIES[at].capability as u16 == capability as u16 {
            return CAPABILITIES[at].needs;
        }
        at += 1;
    }
    panic!("the table holds every capability")
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

    use crate::capability_set::CapabilitySet;
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
    }

    impl CapabilitySet {
        /// The names of the component types a unit of `world`, an entity with a stable id, holds
        /// or held, that no list of these capabilities declares as state or as derived, sorted:
        /// each a part of a unit that no hash, snapshot or client holds. The stable id is the
        /// entity list's, which the state registry holds.
        pub fn unlisted_components(self, world: &World) -> Vec<String> {
            let mut listed = ListedTypes::default();
            self.state_types(&mut listed);
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
pub(crate) mod test_match;

#[cfg(test)]
mod tests;
