use bevy_ecs::world::World;
use std::sync::Arc;

use campfire_actions::{
    ActionBook, ActionData, ActionError, ActionNames, ActionParts, Actions, DeliveryAim,
    DeliveryAims, PackageAction,
};
use campfire_areas::AreaSpec;
use campfire_combat::Combat;
use campfire_common::DeclaredName;
use campfire_declarations::internals::BookLoad;
use campfire_declarations::{Book, ById, ByIdBuilder};
use campfire_production::Production;
use campfire_projectiles::ProjectileSpec;
use campfire_script::{MatchNames, ScriptId};
use campfire_sim::MatchRate;
use campfire_stats::{ModifierBook, Stats};
use campfire_units::ScriptBook;
use campfire_values::{ActionId, PackageIndex, Param, UnitType};

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
        let names = world.resource::<MatchNames>();
        // A test loads modifiers into their book alone.
        let mut tables = names.tables().clone();
        tables.set(Arc::clone(world.resource::<ModifierBook>().names()));
        // How each delivery type the test loaded aims, as the modules that deliver them fill it.
        let mut aims = ByIdBuilder::default();
        let specs = world.get_resource::<ById<UnitType, ProjectileSpec>>();
        for (unit_type, spec) in specs.into_iter().flat_map(ById::iter) {
            aims.set(unit_type, spec.flight().aim());
        }
        let areas = world.get_resource::<ById<UnitType, AreaSpec>>();
        for (unit_type, _) in areas.into_iter().flat_map(ById::iter) {
            aims.set(unit_type, DeliveryAim::Placed);
        }
        let aims = DeliveryAims(aims.build());
        let names = ActionNames::new(&tables, names.tags(), Some(&aims));
        let parts = ActionParts::of(data, package, ranks, rate, &names)?;
        let id = ActionBook::amend(world, |world, actions| {
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
        let places = Stats::stat_places(world, data.keys.params.values().flat_map(Param::stats));
        Stats::load_action_params(world, id, &data.keys.params, |stat| places[stat]);
        ActionBook::share_in(world);
        Ok(id)
    }
}
