use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{NonSendMut, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_script::rhai::Dynamic;
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::{SimSet, StateRegistry, TickRate};

use crate::scripts::ctx::Ctx;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::script_view::View;
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::metric::Metric;

pub(crate) mod action_id;
pub(crate) mod bits256;
pub(crate) mod block;
pub(crate) mod body;
pub(crate) mod body_grid;
pub(crate) mod by_type;
pub(crate) mod collision_data;
pub(crate) mod dead;
pub(crate) mod engine_tag;
pub(crate) mod filter;
pub(crate) mod hit_handle;
pub(crate) mod layer;
pub(crate) mod living_unit;
pub(crate) mod modifier_id;
pub(crate) mod owner;
pub(crate) mod path_id;
pub(crate) mod position_api;
pub(crate) mod predicting;
pub(crate) mod relations;
pub(crate) mod row_fill;
pub(crate) mod script_view;
pub(crate) mod spawn_point;
pub(crate) mod spawner;
pub(crate) mod tag;
pub(crate) mod tag_book;
pub(crate) mod tag_data;
pub(crate) mod tag_effect;
pub(crate) mod tag_effects;
pub(crate) mod tag_set;
pub(crate) mod team;
pub(crate) mod team_set;
pub(crate) mod teams;
pub(crate) mod track_id;
pub(crate) mod type_scope;
pub(crate) mod unit;
pub(crate) mod unit_row;
pub(crate) mod unit_tags;
pub(crate) mod unit_type;
pub(crate) mod unit_type_data;
pub(crate) mod unit_types;
pub(crate) mod view_column;

/// The core's systems, for the capabilities above it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UnitsSet {
    /// In `SimSet::Inputs`: every pool starts full.
    BeginTick,
}

/// The core under every capability's scripts: unit types, the script host they run in, and the
/// units as scripts see them, with the queries on them. Every match installs it before its
/// capabilities.
#[derive(Debug)]
pub struct Units;

impl Units {
    /// Adds the core to a match, on a planar map until the mode sets its own. With `budgets`,
    /// scripts run within them: in Inputs, every pool starts full and the last tick's failures
    /// clear. A client runs no scripts.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        budgets: Option<ScriptBudgets>,
    ) {
        let rate = *world.resource::<TickRate>();
        let view = View::new(rate);
        registry.register_component::<Body>();
        registry.register_component::<Owner>();
        registry.register_component::<SpawnPoint>();
        registry.register_component::<Team>();
        registry.register_component::<UnitType>();
        world.insert_resource(Relations::default());
        registry.register_resource::<Relations>();
        world.insert_resource(Metric::default());
        world.insert_resource(ScriptBook::default());
        let Some(budgets) = budgets else {
            world.insert_non_send(view);
            return;
        };
        let ctx = Ctx::new(view.clone());
        let mut host = ScriptHost::new(budgets.limits().per_call);
        host.engine_mut()
            .set_default_tag(Dynamic::from(ctx.clone()));
        world.insert_non_send(ctx);
        world.insert_non_send(view);
        world.insert_non_send(host);
        world.insert_non_send(ScriptFailures::default());
        world.insert_resource(budgets);
        schedule.add_systems(
            begin_tick
                .in_set(SimSet::Inputs)
                .in_set(UnitsSet::BeginTick),
        );
    }

    /// Compiles `source` in the match's script host, once for every capability that runs it.
    pub fn compile(world: &mut World, source: &str) -> Result<ScriptId, ScriptError> {
        let script = world.non_send_mut::<ScriptHost>().compile(source)?;
        world.resource_scope(|world, mut scripts: Mut<'_, ScriptBook>| {
            debug_assert_eq!(script.index(), scripts.len(), "the book holds every script");
            scripts.push(world.non_send::<ScriptHost>().functions(script));
        });
        Ok(script)
    }
}

fn begin_tick(
    mut budgets: ResMut<'_, ScriptBudgets>,
    mut failures: NonSendMut<'_, ScriptFailures>,
) {
    budgets.begin_tick();
    failures.clear();
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::stats::stats_column::StatsColumn;
    use std::sync::Arc;

    use crate::units::Units;
    use crate::units::script_view::View;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;

    impl Units {
        /// Names, for the scripts of a match with no mode, its `damage_kinds`, its `pools` and its
        /// players' `resources`, each by id, as a mode's books name them.
        pub(crate) fn name_kinds(
            world: &World,
            damage_kinds: &[&str],
            pools: &[&str],
            resources: &[&str],
        ) {
            let names = |names: &[&str]| -> Arc<[DeclaredName]> {
                names
                    .iter()
                    .map(|name| DeclaredName::new(name).expect("a test names a name"))
                    .collect()
            };
            let view = world.non_send::<View>();
            view.set_mode_names(&names(damage_kinds), names(resources));
            StatsColumn::share_pool_names(view, names(pools));
        }

        /// Declares every tag the match's packages name, in their order, after the engine's tags,
        /// before any type, modifier or filter names one, so the tags are numbered the same however
        /// the packages load.
        pub(crate) fn declare_tags<'a>(
            world: &mut World,
            names: impl IntoIterator<Item = &'a str>,
        ) {
            let view = world.non_send::<View>();
            let mut types = view.types_mut();
            for name in names {
                types.declare(name);
            }
        }

        /// Loads the unit type `name` of `scope`, with its core fields: its tags and its params. A
        /// name is one type's only in its scope.
        pub(crate) fn load_type(
            world: &mut World,
            scope: TypeScope,
            name: &str,
            data: &UnitTypeData,
        ) -> UnitType {
            let view = world.non_send::<View>();
            let unit_type = view.types_mut().load(scope, name, data);
            view.share_type_names();
            unit_type
        }

        /// Loads `data` as the next unit type of the mode, named for its place.
        #[cfg(test)]
        pub(crate) fn load_next_type(world: &mut World, data: &UnitTypeData) -> UnitType {
            let name = format!("type {}", world.non_send::<View>().types_count());
            Units::load_type(world, TypeScope::Mode, &name, data)
        }
    }
}

#[cfg(test)]
mod tests;
