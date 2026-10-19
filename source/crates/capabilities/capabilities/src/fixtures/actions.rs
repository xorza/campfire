use bevy_ecs::change_detection::Mut;
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionData, ActionError, ActionNames, ActionParts, Actions, CostTarget,
    PackageAction, ProjectileFlight,
};
use campfire_combat::Combat;
use campfire_common::DeclaredName;
use campfire_declare::{Book, ByType};
use campfire_production::Production;
use campfire_projectiles::ProjectileSpec;
use campfire_script::{Filter, MatchNames, ScriptId, UnknownName};
use campfire_sim::MatchRate;
use campfire_stats::{ModifierBook, Stats};
use campfire_units::ScriptBook;
use campfire_values::{
    ActionId, FilterData, ModifierId, Named, PackageIndex, Param, TypeScope, UnitType,
};

use crate::fixtures::units::ViewFixtures;

/// The names of an action's data as a match's world resolves them: its names, its book of
/// modifiers, and the world's projectile specs.
#[derive(Debug)]
pub struct WorldActionNames<'w> {
    world: &'w World,
    names: &'w MatchNames,
    modifiers: &'w ModifierBook,
}

impl<'w> WorldActionNames<'w> {
    pub fn new(world: &'w World) -> Self {
        WorldActionNames {
            world,
            names: world.resource::<MatchNames>(),
            modifiers: world.resource::<ModifierBook>(),
        }
    }
}

impl ActionNames for WorldActionNames<'_> {
    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        self.names.cost_target(name.as_str())
    }

    fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
        self.names.resolve_filter(filter)
    }

    fn modifier(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<ModifierId, UnknownName> {
        self.modifiers
            .names()
            .named((package, name.as_str()))
            .ok_or_else(|| UnknownName::new(ModifierId::NAMES, name))
    }

    fn flight(&self, package: PackageIndex, name: &DeclaredName) -> Option<ProjectileFlight> {
        let unit_type = self
            .unit_type(package, name)
            .expect("a test loads it first");
        let specs = self.world.resource::<ByType<ProjectileSpec>>();
        let spec = specs.get(unit_type);
        Some(
            spec.expect("a test loads a projectile type's spec before its action")
                .flight(),
        )
    }

    fn unit_type(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<UnitType, UnknownName> {
        let scope = TypeScope::of_package(package);
        self.names
            .named((scope, name.as_str()))
            .ok_or_else(|| UnknownName::new(UnitType::NAMES, name))
    }
}

/// The test fixtures of `Actions` that need a crate above its own.
pub trait ActionsFixtures {
    /// Loads the action `name` of `package`, of `ranks` ranks, into the match, which the
    /// package load checked, with its compiled script exactly when its data names one: its
    /// capability fields at each rank, times in milliseconds as ticks at the match's rate,
    /// rounded up, and an attack's weapon and a train's unit type into their books.
    fn load(
        world: &mut World,
        package: PackageIndex,
        name: &str,
        data: &ActionData,
        script: Option<ScriptId>,
        ranks: u8,
    ) -> Result<ActionId, ActionError>;
}

impl ActionsFixtures for Actions {
    fn load(
        world: &mut World,
        package: PackageIndex,
        name: &str,
        data: &ActionData,
        script: Option<ScriptId>,
        ranks: u8,
    ) -> Result<ActionId, ActionError> {
        let rate = *world.resource::<MatchRate>();
        let names = WorldActionNames::new(world);
        let parts = ActionParts::of(data, package, ranks, rate, &names)?;
        let id = world.resource_scope(|world, mut actions: Mut<'_, ActionBook>| {
            // A match that runs no scripts holds no script book, and its actions none.
            let none = ScriptBook::default();
            let scripts = world.get_resource::<ScriptBook>().unwrap_or(&none);
            let action = PackageAction {
                package,
                name,
                data,
                script,
                parts,
            };
            actions.load(scripts, action)
        });
        let id_name = DeclaredName::new(name).expect("a test's action is named as data names it");
        Combat::load_weapon(world, id, data, &id_name);
        Production::load_train(world, id, data, package);
        let places = Stats::stat_places(world, data.params.values().flat_map(Param::stats));
        Stats::load_action_params(world, id, &data.params, |stat| places[stat]);
        ActionBook::share_in(world);
        Ok(id)
    }
}
