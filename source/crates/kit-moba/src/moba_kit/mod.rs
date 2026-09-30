use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Query, Res};
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::{IdAllocator, Position, SimSet, StableId, StateRegistry, TickInputs};

use crate::controller::Controller;
use crate::destination::Destination;
use crate::move_step::MoveStep;
use crate::order::Order;

/// Wires the MOBA kit into a sim: its systems, its state types, and its units.
#[derive(Debug)]
pub struct MobaKit;

impl MobaKit {
    pub fn add_systems(schedule: &mut Schedule) {
        schedule.add_systems((
            apply_orders.in_set(SimSet::Inputs),
            move_units.in_set(SimSet::BeforeCollision),
        ));
    }

    pub fn register_state(registry: &mut StateRegistry) {
        registry.register_component::<Controller>();
        registry.register_component::<Destination>();
        registry.register_component::<MoveStep>();
    }

    /// Spawns a hero that follows the orders of the player in `slot`.
    pub fn spawn_hero(world: &mut World, slot: u32, at: Position, step: MoveStep) -> StableId {
        let id = world.resource_mut::<IdAllocator>().allocate();
        world.spawn((id, at, Controller::new(slot), Destination::default(), step));
        id
    }
}

/// Makes each tick input the current order of its player's units, in input order, so a later
/// order in the tick wins. A payload that is not an order, or a target beyond the world bound, is
/// ignored: a client can send anything.
fn apply_orders(
    inputs: Res<'_, TickInputs>,
    mut units: Query<'_, '_, (&Controller, &Position, &mut Destination)>,
) {
    for input in inputs.iter() {
        let Some(Order::Move { x, z }) = Order::decode(input.payload) else {
            continue;
        };
        for (controller, position, mut destination) in &mut units {
            if controller.slot() != input.slot {
                continue;
            }
            // Orders name a point on the ground plane; the unit keeps its height.
            if let Some(target) = Position::new(Vec3::new(x, position.get().y, z)) {
                destination.set(Some(target));
            }
        }
    }
}

fn move_units(mut units: Query<'_, '_, (&mut Position, &mut Destination, &MoveStep)>) {
    for (mut position, mut destination, step) in &mut units {
        let Some(target) = destination.get() else {
            continue;
        };
        let moved = position.get().step_toward(target.get(), step.get());
        *position = Position::new(moved).expect("a step ends between two points within the bound");
        if moved == target.get() {
            destination.set(None);
        }
    }
}

#[cfg(test)]
mod tests;
