use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::message::{Message, MessageReader, Messages};
use bevy_ecs::observer::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{ResMut, SystemParam};
use bevy_ecs::world::World;

use crate::sim_update::TickEndSystems;

/// The removals of `C` that a system has not read since its last run, of this tick and the one
/// before, as the tick ages them (02-engine-core.md, A tick's removals are the tick's). Bevy's
/// `RemovedComponents` ages with each update of the app that holds the world, many a tick on a
/// server, one in a replay, so a removal after its reader ran would reach the replay's reader
/// alone.
#[derive(SystemParam, Debug)]
pub struct Removals<'w, 's, C: Component> {
    removed: MessageReader<'w, 's, Removed<C>>,
}

impl<C: Component> Removals<'_, '_, C> {
    /// Each entity that lost its `C` since the system's last run, in the order it lost it.
    pub fn read(&mut self) -> impl ExactSizeIterator<Item = Entity> + '_ {
        self.removed.read().map(|removed| removed.entity)
    }

    /// Records each removal of `C` in `world` from now on, and ages the records as each tick of
    /// `schedule`, the sim's, ends; once for each `C`, however many modules read it.
    pub fn track(world: &mut World, schedule: &mut Schedule) {
        if world.contains_resource::<Messages<Removed<C>>>() {
            return;
        }
        world.init_resource::<Messages<Removed<C>>>();
        world.add_observer(
            |removal: On<'_, '_, Remove<C>>, mut removed: ResMut<'_, Messages<Removed<C>>>| {
                removed.write(Removed {
                    entity: removal.entity,
                    part: PhantomData,
                });
            },
        );
        schedule.add_systems(Self::age.in_set(TickEndSystems));
    }

    fn age(mut removed: ResMut<'_, Messages<Removed<C>>>) {
        removed.update();
    }
}

/// A removal of a `C` from an entity.
pub struct Removed<C> {
    entity: Entity,
    part: PhantomData<fn() -> C>,
}

impl<C: Component> Message for Removed<C> {}

impl<C> fmt::Debug for Removed<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Removed").field(&self.entity).finish()
    }
}

#[cfg(test)]
mod tests;
