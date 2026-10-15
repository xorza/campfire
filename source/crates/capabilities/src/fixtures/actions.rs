use bevy_ecs::change_detection::Mut;
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionData, ActionError, ActionNames, ActionParts, Actions, CostTarget,
};
use campfire_declare::{Book, ByType};
use campfire_projectiles::ProjectileSpec;
use campfire_script::{Filter, NameKind, ScriptId, UnknownName, View};
use campfire_sim::TickRate;
use campfire_stats::{ModifierBook, StatId, Stats};
use campfire_units::ScriptBook;
use campfire_values::{
    ActionId, DamageKind, DeclaredName, FilterData, ModifierId, PackageIndex, Param, Stat,
    TypeScope, UnitType,
};

use crate::fixtures::units::ViewFixtures;

/// The names of an action's data as a match's world resolves them: its view, its book of
/// modifiers, and the world's projectile specs.
#[derive(Debug)]
pub struct MatchNames<'w> {
    world: &'w World,
    view: &'w View,
    modifiers: &'w ModifierBook,
}

impl<'w> MatchNames<'w> {
    pub const fn new(world: &'w World, view: &'w View, modifiers: &'w ModifierBook) -> Self {
        MatchNames {
            world,
            view,
            modifiers,
        }
    }
}

impl ActionNames for MatchNames<'_> {
    fn stat(&self, stat: &Stat) -> Result<StatId, UnknownName> {
        self.view
            .stat_index(stat)
            .ok_or_else(|| UnknownName::new(NameKind::Stat, stat))
    }

    fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
        self.view
            .names()
            .named(name.as_str())
            .ok_or_else(|| UnknownName::new(NameKind::DamageKind, name))
    }

    fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        self.view.cost_target(name.as_str())
    }

    fn filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
        self.view.resolve_filter(filter)
    }

    fn modifier(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<ModifierId, UnknownName> {
        self.modifiers
            .names()
            .named((package, name.as_str()))
            .ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
    }

    fn homes(&self, package: PackageIndex, name: &DeclaredName) -> Option<bool> {
        let unit_type = self
            .unit_type(package, name)
            .expect("a test loads it first");
        let specs = self.world.resource::<ByType<ProjectileSpec>>();
        let spec = specs.get(unit_type);
        Some(
            spec.expect("a test loads a projectile type's spec before its action")
                .homing,
        )
    }

    fn unit_type(
        &self,
        package: PackageIndex,
        name: &DeclaredName,
    ) -> Result<UnitType, UnknownName> {
        let scope = TypeScope::of_package(package);
        self.view
            .names()
            .named((scope, name.as_str()))
            .ok_or_else(|| UnknownName::new(NameKind::UnitType, name))
    }
}

/// The test fixtures of `Actions` that need a crate above its own.
pub trait ActionsFixtures {
    /// Loads the action `name` of `package`, of `ranks` ranks, into the match, which the
    /// package load checked, with its compiled script exactly when its data names one: its
    /// capability fields at each rank, times in milliseconds as ticks at the match's rate,
    /// rounded up.
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
        let rate = *world.resource::<TickRate>();
        let view = world.non_send::<View>().clone();
        let names = MatchNames::new(world, &view, world.resource::<ModifierBook>());
        let parts = ActionParts::of(data, package, ranks, rate, &names)?;
        let id = world.resource_scope(|world, mut actions: Mut<'_, ActionBook>| {
            // A match that runs no scripts holds no script book, and its actions none.
            let none = ScriptBook::default();
            let scripts = world.get_resource::<ScriptBook>().unwrap_or(&none);
            actions.load(scripts, package, name, data, script, parts)
        });
        let places = Stats::stat_places(world, data.params.values().flat_map(Param::stats));
        Stats::load_action_params(world, id, &data.params, |stat| places[stat]);
        ActionBook::share_in(world);
        Ok(id)
    }
}
