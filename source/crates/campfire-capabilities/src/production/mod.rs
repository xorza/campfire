use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_sim::{
    IdAllocator, Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry,
};

use crate::actions::ActionsSet;
use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::{ActionSlots, InProgress, OrderPhase, SlotAim};
use crate::actions::kind_spec::KindSpec;
use crate::actions::purse::{Payer, Purse};
use crate::players::player_resources::PlayerResources;
use crate::production::production_data::ProductionData;
use crate::production::train_queue::{Queued, TrainQueue};
use crate::stats::pools::Pools;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::spawner::{SpawnAt, Spawner};
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;

pub(crate) mod production_api;
pub(crate) mod production_data;
pub(crate) mod train_queue;

/// The `production` capability: units that train others, through a queue.
#[derive(Debug)]
pub struct Production;

/// The systems of `production`, for the mode to order its own against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProductionSet {
    /// In `SimSet::Mode`: the trains whose time ended spawn.
    Finish,
}

impl Production {
    /// Adds production to a match: in Act, after the other orders start, ordered trains pass their
    /// checks, pay, and join their unit's queue; in Mode, before the mode's hooks, the trains whose
    /// time ended spawn.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(ByType::<ProductionData>::default());
        schedule.add_systems((
            start_trains.in_set(SimSet::Act).after(ActionsSet::Start),
            Production::finish_trains
                .in_set(SimSet::Mode)
                .in_set(ProductionSet::Finish),
        ));
        registry.register_component::<TrainQueue>();
    }

    /// Spawns each train whose time ended this tick, at its unit's position, of its unit's team
    /// and player, by its unit's stable id, then its queue's order, through the mode's spawner;
    /// the next in a queue starts in the same tick. A dead unit's queue waits.
    fn finish_trains(
        world: &mut World,
        producers: &mut QueryState<(Entity, &StableId, &TrainQueue), Without<Dead>>,
        mut order: Local<'_, Ordered>,
    ) {
        let now = world.resource::<SimTick>().end();
        let done = producers
            .iter(world)
            .filter(|(.., queue)| queue.done(now).is_some())
            .map(|(entity, &id, _)| Keyed { id, entity });
        let due = order.sort(done);
        if due.is_empty() {
            return;
        }
        let spawner = world.non_send::<Spawner>().clone();
        for &Keyed { entity, .. } in due {
            while let Some(head) = world
                .get::<TrainQueue>(entity)
                .and_then(|queue| queue.done(now))
            {
                let action = world.resource::<ActionBook>().get(head.action);
                let Some(KindSpec::Train(unit_type)) = action.map(|action| action.kind) else {
                    panic!("a train queues only trains");
                };
                let producer = world.entity(entity);
                let team = *producer.get::<Team>().expect("a producer has a team");
                let pos = *producer.get::<Position>().expect("a producer stands");
                let owner = producer.get::<Owner>().map(|owner| owner.slot());
                let id = world.resource_mut::<IdAllocator>().allocate();
                let at = SpawnAt {
                    id,
                    unit_type,
                    team,
                    pos,
                };
                spawner.spawn(world, at, owner);
                let mut queue = world.get_mut::<TrainQueue>(entity).expect("a producer");
                queue.pop(now);
            }
        }
    }

    /// The train `slots` hold ordered, not yet checked, whose action `book` says trains.
    fn ordered(slots: &ActionSlots, book: &ActionBook) -> Option<SlotAim> {
        let Some(InProgress::Order {
            aim,
            phase: OrderPhase::Ordered,
        }) = slots.in_progress()
        else {
            return None;
        };
        let slot = slots.slot(aim.slot)?;
        let trains = book.get(slot.action)?.kind.kind() == ActionKind::Train;
        trains.then_some(aim)
    }
}

/// Starts each ordered train, in Act: one that passes the core's checks, and finds a place in
/// its unit's queue, pays its whole cost, in pools and player resources, goes on cooldown, and
/// joins the queue. The order ends either way, and the unit stays free.
fn start_trains(
    tick: Res<'_, SimTick>,
    (book, producers): (Res<'_, ActionBook>, Res<'_, ByType<ProductionData>>),
    mut resources: Option<ResMut<'_, PlayerResources>>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &UnitType,
            &mut ActionSlots,
            &mut TrainQueue,
            Option<&mut Pools>,
            Option<&Owner>,
        ),
        Without<Dead>,
    >,
    mut order: Local<'_, Ordered>,
) {
    let now = tick.start();
    let trains = units.iter().filter_map(|(entity, &id, _, slots, ..)| {
        Production::ordered(slots, &book).map(|_| Keyed { id, entity })
    });
    for &Keyed { entity, .. } in order.sort(trains) {
        let (_, _, &unit_type, mut slots, mut queue, mut pools, owner) =
            units.get_mut(entity).expect("a unit in the order");
        let ordered = Production::ordered(&slots, &book).expect("an ordered train");
        slots.stop();
        let owner = owner.map(|owner| owner.slot());
        let purse = Purse {
            pools: pools.as_deref(),
            resources: resources.as_deref(),
            owner,
        };
        let no_target = |_| Attitude::Friendly;
        let Some(checked) = book.check(now, &slots, purse, ordered, no_target, |_| None) else {
            continue;
        };
        let capacity = producers.get(unit_type).map(|production| production.queue);
        if !capacity.is_some_and(|capacity| queue.has_room(capacity)) {
            continue;
        }
        let values = checked.values;
        let payer = Payer {
            pools: pools.as_deref_mut(),
            resources: resources.as_deref_mut(),
            owner,
        };
        payer.pay(&values.cost, checked.action.resource_cost(checked.rank));
        let queued = Queued {
            action: checked.id,
            rank: checked.rank,
            time: values.windup,
        };
        slots.cool_down(ordered.slot, now.after(values.cooldown));
        queue.push(queued, now);
    }
}

#[cfg(test)]
mod tests;
