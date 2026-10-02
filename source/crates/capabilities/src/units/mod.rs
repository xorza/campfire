use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{NonSendMut, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_script::rhai::Dynamic;
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::{SimSet, StateRegistry, TickRate};

use crate::actions::Actions;
use crate::scripts::ctx::Ctx;
use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::units::body::Body;
use crate::units::error::UnitTypeError;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::script_view::View;
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::metric::Metric;

pub(crate) mod block;
pub(crate) mod body;
pub(crate) mod by_type;
pub(crate) mod collision_data;
pub(crate) mod engine_tag;
pub(crate) mod error;
pub(crate) mod filter;
pub(crate) mod hit_handle;
pub(crate) mod layer;
pub(crate) mod living_unit;
pub(crate) mod owner;
pub(crate) mod path_id;
pub(crate) mod recent_attack;
pub(crate) mod relations;
pub(crate) mod script_view;
pub(crate) mod spawn_point;
pub(crate) mod tag;
pub(crate) mod tag_book;
pub(crate) mod tag_data;
pub(crate) mod tag_effect;
pub(crate) mod tag_effects;
pub(crate) mod tag_set;
pub(crate) mod team;
pub(crate) mod team_set;
pub(crate) mod teams;
pub(crate) mod type_scope;
pub(crate) mod unit;
pub(crate) mod unit_tags;
pub(crate) mod unit_type;
pub(crate) mod unit_type_data;
pub(crate) mod unit_types;

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
    /// Adds the core to a match, on a planar map until the mode sets its own. With `scripts`,
    /// scripts run within their limits: in Inputs,
    /// every pool starts full and the last tick's failures clear. A client runs no scripts.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        scripts: Option<MatchScripts>,
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
        Actions::install(world, registry, &view);
        world.insert_resource(Metric::default());
        let Some(MatchScripts {
            limits,
            players,
            damage_kinds,
            stats,
            pools,
            resources,
        }) = scripts
        else {
            world.insert_non_send(view);
            world.insert_resource(ScriptBook::default());
            return;
        };
        view.set_damage_kinds(damage_kinds);
        view.set_stat_names(stats);
        view.set_pool_names(pools);
        view.set_resource_names(resources);
        let ctx = Ctx::new(view.clone());
        let mut host = ScriptHost::new(limits.per_call);
        ScriptApi::bind(host.engine_mut());
        host.engine_mut()
            .set_default_tag(Dynamic::from(ctx.clone()));
        world.insert_non_send(ctx);
        world.insert_non_send(view);
        world.insert_non_send(host);
        world.insert_resource(ScriptBook::default());
        world.insert_non_send(ScriptFailures::default());
        world.insert_resource(ScriptBudgets::new(limits, players));
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

    /// Declares every tag the match's packages name, in their order, after the engine's tags,
    /// before any type, modifier or filter names one, so the tags are numbered the same however
    /// the packages load.
    pub fn declare_tags<'a>(
        world: &mut World,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), UnitTypeError> {
        let view = world.non_send::<View>();
        let mut types = view.types_mut();
        for name in names {
            types.declare(name)?;
        }
        Ok(())
    }

    /// Gives the match its tags' effects and its unit types' own tags.
    pub(crate) fn load_tags(world: &mut World, book: TagBook) {
        world.insert_resource(book);
    }

    /// Loads the unit type `name` of `scope`, with its core fields: its tags and its params. A
    /// name is one type's only in its scope.
    pub fn load_type(
        world: &mut World,
        scope: TypeScope,
        name: &str,
        data: &UnitTypeData,
    ) -> Result<UnitType, UnitTypeError> {
        world.non_send::<View>().types_mut().load(scope, name, data)
    }
}

fn begin_tick(
    mut budgets: ResMut<'_, ScriptBudgets>,
    mut failures: NonSendMut<'_, ScriptFailures>,
) {
    budgets.begin_tick();
    failures.0.clear();
}

#[cfg(test)]
mod tests;
