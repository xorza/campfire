//! The index of the units whose standing order runs in each tick, as a builder's build or a
//! gatherer's loop: each is due at once while it has its order, and stays held until it ends
//! (04-capabilities/00-overview.md, One due index for each timed process).

use bevy_ecs::change_detection::Mut;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{DueIndex, DueUnit, StableId, TimedProcess, UnitDues};

/// A part that holds a unit's standing order: its unit is due at once while it holds one.
pub(crate) trait StandingOrder:
    Component<Mutability = Mutable> + TimedProcess<Parts = &'static Self, Books = ()>
{
    fn has_order(&self) -> bool;
}

/// Runs `change` on the part `P` of `entity` in `world`, with its dues, as an exclusive system
/// changes its order.
pub(crate) fn change<P: StandingOrder>(
    world: &mut World,
    entity: Entity,
    change: impl FnOnce(&mut P, &mut UnitDues<'_, P>),
) {
    let id = *world.get::<StableId>(entity).expect("a unit has an id");
    world.resource_scope(|world, mut index: Mut<'_, DueIndex<P>>| {
        let mut part = world
            .get_mut::<P>(entity)
            .expect("a unit with its order's part");
        change(&mut part, &mut UnitDues::new(&mut index, id, entity));
    });
}

/// Collects into `held` each unit the index of `P` holds, by stable id, and forgets each whose
/// order ended or that is gone.
pub(crate) fn held<P: StandingOrder>(world: &mut World, held: &mut Vec<DueUnit>) {
    held.clear();
    world.resource_scope(|world, mut index: Mut<'_, DueIndex<P>>| {
        index.peek_due(Tick::ZERO, held);
        held.retain(|unit| {
            let ordered = world.get::<P>(unit.entity).is_some_and(P::has_order);
            if !ordered {
                index.forget(unit.entity);
            }
            ordered
        });
    });
}
