use bevy_ecs::query::Has;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::Query;
use bevy_ecs::world::{EntityRef, World};
use campfire_sim::{Position, SimSet, StateRegistry};

use crate::combat::dead::Dead;
use crate::navigation::destination::Destination;
use crate::navigation::move_step::MoveStep;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::units::script_view::{RowFill, View};

pub(crate) mod destination;
pub(crate) mod move_step;
pub(crate) mod on_path;
pub(crate) mod path_walker;
pub(crate) mod paths;

/// The `navigation` capability: units that walk to a destination, and the map's waypoint paths.
#[derive(Debug)]
pub struct Navigation;

impl Navigation {
    /// Adds navigation to a match, with no paths until the mode sets its map's: in Move, units
    /// walk towards their destination.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(Paths::default());
        schedule.add_systems(move_units.in_set(SimSet::Move));
        registry.register_component::<Destination>();
        registry.register_component::<PathWalker>();
        registry.register_component::<MoveStep>();
        registry.register_component::<OnPath>();
    }
}

/// Fills a row of the script view with the path the unit walks or stands on.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.path = unit.get::<OnPath>().map(|path| path.get());
}

/// Walks each unit one step towards its destination, which it drops on arrival. A dead unit stays
/// where it fell, and forgets where it walked to.
fn move_units(mut units: Query<'_, '_, (&mut Position, &mut Destination, &MoveStep, Has<Dead>)>) {
    for (mut position, mut destination, step, dead) in &mut units {
        let Some(target) = destination.get() else {
            continue;
        };
        if dead {
            destination.set(None);
            continue;
        }
        let moved = position.get().step_toward(target.get(), step.get());
        *position = Position::new(moved).expect("a step ends between two points within the bound");
        if moved == target.get() {
            destination.set(None);
        }
    }
}

#[cfg(test)]
mod tests;
