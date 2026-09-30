use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{NonSendMut, ResMut};
use bevy_ecs::world::World;
use campfire_script::ScriptHost;
use campfire_sim::{SimSet, StateRegistry, TickRate};

use crate::units::error::UnitTypeError;
use crate::units::script_budgets::ScriptBudgets;
use crate::units::script_failures::ScriptFailures;
use crate::units::script_limits::ScriptLimits;
use crate::units::script_view::View;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;

pub(crate) mod error;
pub(crate) mod filter;
pub(crate) mod relation;
pub(crate) mod scalar;
pub(crate) mod script_budgets;
pub(crate) mod script_failures;
pub(crate) mod script_limits;
pub(crate) mod script_view;
pub(crate) mod tag_set;
pub(crate) mod unit;
pub(crate) mod unit_type;
pub(crate) mod unit_type_data;
pub(crate) mod unit_types;

/// The core under every capability's scripts: unit types, the script host they run in, and the
/// units as scripts see them, with the queries on them. Every match installs it before its
/// capabilities.
#[derive(Debug)]
pub struct Units;

impl Units {
    /// Adds the core to a match, scripts running within `limits`: in Inputs, every pool starts
    /// full and the last tick's failures clear.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        limits: ScriptLimits,
    ) {
        assert!(
            limits.input >= limits.per_call && limits.think >= limits.per_call,
            "a pool holds a whole call"
        );
        let rate = *world.resource::<TickRate>();
        let mut host = ScriptHost::new(limits.per_call);
        Unit::register(host.engine_mut());
        world.insert_non_send(host);
        world.insert_non_send(View::new(rate));
        world.insert_non_send(ScriptFailures::default());
        world.insert_resource(ScriptBudgets::new(limits));
        schedule.add_systems(begin_tick.in_set(SimSet::Inputs));
        registry.register_component::<UnitType>();
    }

    /// Loads a unit type's core fields into the match: its tags and its params.
    pub fn load_type(world: &mut World, data: &UnitTypeData) -> Result<UnitType, UnitTypeError> {
        world.non_send::<View>().types_mut().load(data)
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
