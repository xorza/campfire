use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{NonSendMut, ResMut};
use bevy_ecs::world::World;
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::{SimSet, StateRegistry, TickRate};

use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::units::error::UnitTypeError;
use crate::units::owner::Owner;
use crate::units::script_view::View;
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;

pub(crate) mod by_type;
pub(crate) mod error;
pub(crate) mod filter;
pub(crate) mod living_unit;
pub(crate) mod owner;
pub(crate) mod path_id;
pub(crate) mod recent_attack;
pub(crate) mod script_view;
pub(crate) mod spawn_point;
pub(crate) mod tag_set;
pub(crate) mod team;
pub(crate) mod team_set;
pub(crate) mod teams;
pub(crate) mod unit;
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
    /// Adds the core to a match. With `scripts`, scripts run within their limits: in Inputs,
    /// every pool starts full and the last tick's failures clear. A client runs no scripts.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        scripts: Option<MatchScripts>,
    ) {
        let rate = *world.resource::<TickRate>();
        let view = View::new(rate);
        registry.register_component::<Owner>();
        registry.register_component::<SpawnPoint>();
        registry.register_component::<Team>();
        registry.register_component::<UnitType>();
        let Some(MatchScripts {
            limits,
            players,
            damage_kinds,
        }) = scripts
        else {
            world.insert_non_send(view);
            return;
        };
        view.set_damage_kinds(damage_kinds);
        world.insert_non_send(view);
        let mut host = ScriptHost::new(limits.per_call);
        Unit::register(host.engine_mut());
        world.insert_non_send(host);
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
        world.non_send_mut::<ScriptHost>().compile(source)
    }

    /// Loads the unit type `name`, with its core fields: its tags and its params. A name is one
    /// type's only.
    pub fn load_type(
        world: &mut World,
        name: &str,
        data: &UnitTypeData,
    ) -> Result<UnitType, UnitTypeError> {
        world.non_send::<View>().types_mut().load(name, data)
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
