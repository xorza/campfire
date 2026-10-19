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

use crate::sim_update::TickEnd;

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
            |removal: On<'_, '_, Remove, C>, mut removed: ResMut<'_, Messages<Removed<C>>>| {
                removed.write(Removed {
                    entity: removal.entity,
                    part: PhantomData,
                });
            },
        );
        schedule.add_systems(Self::age.in_set(TickEnd));
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
mod tests {
    use std::num::NonZeroU32;

    use bevy_ecs::lifecycle::RemovedComponents;
    use bevy_ecs::query::With;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::{Commands, Query};
    use campfire_common::SegmentSeed;

    use super::*;
    use crate::match_rate::MatchRate;
    use crate::sim_update::{SimSet, SimUpdate};

    #[derive(Component, Debug)]
    struct Mark;

    /// What the reader read at each tick: through `Removals`, and through Bevy's
    /// `RemovedComponents`.
    #[derive(Resource, Debug, Default)]
    struct Seen {
        by_tick: Vec<usize>,
        by_app: Vec<usize>,
    }

    fn read(
        mut removals: Removals<'_, '_, Mark>,
        mut app: RemovedComponents<'_, '_, Mark>,
        mut seen: ResMut<'_, Seen>,
    ) {
        seen.by_tick.push(removals.read().count());
        seen.by_app.push(app.read().count());
    }

    /// Removes one mark late in the tick, after the reader ran.
    fn remove_one(marked: Query<'_, '_, Entity, With<Mark>>, mut commands: Commands<'_, '_>) {
        if let Some(entity) = marked.iter().min() {
            commands.entity(entity).remove::<Mark>();
        }
    }

    /// Four ticks of a world whose host clears its trackers `updates` times after each tick, as
    /// an app that runs that many frames a tick does.
    fn host(updates: u32) -> Seen {
        let mut world = World::new();
        let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
        SimUpdate::prepare(&mut world, SegmentSeed::new([1; 32]), rate);
        world.init_resource::<Seen>();
        let mut schedule = SimUpdate::schedule();
        Removals::<Mark>::track(&mut world, &mut schedule);
        Removals::<Mark>::track(&mut world, &mut schedule);
        schedule.add_systems((read.in_set(SimSet::Inputs), remove_one.in_set(SimSet::Mode)));
        world.spawn_batch([Mark, Mark, Mark]);
        for _ in 0..4 {
            schedule.run(&mut world);
            for _ in 0..updates {
                world.clear_trackers();
            }
        }
        world.remove_resource::<Seen>().unwrap()
    }

    #[test]
    fn a_removal_after_its_reader_reaches_it_next_tick_however_often_the_host_updates() {
        // Each tick removes one mark after the reader ran, so the reader of each tick but the
        // first reads one removal, that of the tick before, whatever the host does between ticks.
        let replay = host(1);
        let server = host(3);
        assert_eq!(replay.by_tick, [0, 1, 1, 1]);
        assert_eq!(server.by_tick, replay.by_tick);
        // Bevy's own removals reach a replay's reader, which clears once a tick, and are gone
        // from a server's, which clears three times: the divergence the tick's removals end.
        assert_eq!(replay.by_app, [0, 1, 1, 1]);
        assert_eq!(server.by_app, [0, 0, 0, 0]);
    }
}
