use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::NonSendMut;
use bevy_ecs::world::World;
use campfire_script::{ScriptHost, ScriptLimits};
use campfire_sim::{SimSet, StateRegistry, TickRate};

use crate::units::error::UnitTypeError;
use crate::units::script_failures::ScriptFailures;
use crate::units::script_view::View;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;

pub(crate) mod error;
pub(crate) mod filter;
pub(crate) mod relation;
pub(crate) mod scalar;
pub(crate) mod script_failures;
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
    /// Adds the core to a match of `rate` ticks a second, scripts running within `limits`: in
    /// Inputs, the tick's script budget starts and the last tick's failures clear.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        limits: ScriptLimits,
        rate: TickRate,
    ) {
        let mut host = ScriptHost::new(limits);
        Unit::register(host.engine_mut());
        world.insert_non_send(host);
        world.insert_non_send(View::new(rate));
        world.insert_non_send(ScriptFailures::default());
        world.insert_resource(rate);
        schedule.add_systems(begin_tick.in_set(SimSet::Inputs));
        registry.register_component::<UnitType>();
    }

    /// Loads a unit type's core fields into the match: its tags and its params.
    pub fn load_type(world: &mut World, data: &UnitTypeData) -> Result<UnitType, UnitTypeError> {
        world.non_send::<View>().types_mut().load(data)
    }
}

fn begin_tick(mut host: NonSendMut<'_, ScriptHost>, mut failures: NonSendMut<'_, ScriptFailures>) {
    host.begin_tick();
    failures.0.clear();
}

#[cfg(test)]
mod tests;
