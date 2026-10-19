//! The index of the units whose standing order runs in each tick, as a builder's build or a
//! gatherer's loop: each is due at once while it has its order, and stays held until it ends
//! (04-capabilities/00-overview.md, One due index for each timed process).

use bevy_ecs::change_detection::Mut;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::Tick;
use campfire_sim::{DueIndex, DueUnit, StableId, TimedProcess};

/// A part that holds a unit's standing order: its unit is due at once while it holds one.
pub(crate) trait StandingOrder:
    Component<Mutability = Mutable> + TimedProcess<Parts = &'static Self, Books = ()>
{
    fn has_order(&self) -> bool;
}

/// The index of the standing orders `P`, for the unit `id`, of `entity`, which an order it is
/// given pushes it into.
#[derive(Debug)]
pub(crate) struct OrderDues<'a, P: TimedProcess> {
    index: &'a mut DueIndex<P>,
    id: StableId,
    entity: Entity,
}

impl<'a, P: StandingOrder> OrderDues<'a, P> {
    pub(crate) const fn new(index: &'a mut DueIndex<P>, id: StableId, entity: Entity) -> Self {
        Self { index, id, entity }
    }

    /// Pushes the unit at once, as `part` holds an order.
    pub(crate) fn push(&mut self, part: &P) {
        if part.has_order() {
            self.index.push(Tick::ZERO, self.id, self.entity);
        }
    }
}

/// Runs `change` on the part `P` of `entity` in `world`, with its dues, as an exclusive system
/// changes its order.
pub(crate) fn change<P: StandingOrder>(
    world: &mut World,
    entity: Entity,
    change: impl FnOnce(&mut P, &mut OrderDues<'_, P>),
) {
    let id = *world.get::<StableId>(entity).expect("a unit has an id");
    world.resource_scope(|world, mut index: Mut<'_, DueIndex<P>>| {
        let mut part = world
            .get_mut::<P>(entity)
            .expect("a unit with its order's part");
        change(&mut part, &mut OrderDues::new(&mut index, id, entity));
    });
}

/// Collects into `held` each unit the index of `P` holds, by stable id, and forgets each whose
/// order ended or that is gone.
pub(crate) fn held<P: StandingOrder>(world: &mut World, held: &mut Vec<DueUnit>) {
    held.clear();
    world.resource_scope(|world, mut index: Mut<'_, DueIndex<P>>| {
        held.extend(index.peek_due(Tick::ZERO));
        held.retain(|unit| {
            let ordered = world.get::<P>(unit.entity).is_some_and(P::has_order);
            if !ordered {
                index.forget(unit.id);
            }
            ordered
        });
    });
}

/// Pushes the unit whose part `P` comes, at each insert, while it holds an order.
pub(crate) fn pushed<P: StandingOrder>(mut world: DeferredWorld<'_>, context: HookContext) {
    let entity = context.entity;
    let unit = world.entity(entity);
    let (Some(part), Some(&id)) = (unit.get::<P>(), unit.get::<StableId>()) else {
        return;
    };
    let ordered = part.has_order();
    if let (true, Some(mut index)) = (ordered, world.get_resource_mut::<DueIndex<P>>()) {
        index.push(Tick::ZERO, id, entity);
    }
}
