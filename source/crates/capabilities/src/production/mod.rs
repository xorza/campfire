use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::Ticks;
use campfire_sim::{
    IdAllocator, Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry,
};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::{ActionSlots, InProgress};
use crate::actions::kind_spec::KindSpec;
use crate::actions::purse::{Payer, Purse};
use crate::combat::CombatSet;
use crate::mode::mode_book::SpawnAt;
use crate::players::player_resources::PlayerResources;
use crate::production::train_queue::{Queued, TrainQueue};
use crate::scripts::ctx::Ctx;
use crate::stats::pools::Pools;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::values::attitude::Attitude;

pub(crate) mod production_api;
pub(crate) mod production_data;
pub(crate) mod train_queue;

/// The `production` capability: units that train others, through a queue.
#[derive(Debug)]
pub struct Production;

impl Production {
    /// Adds production to a match: in Act, after attacks start, ordered trains pass their checks,
    /// pay, and join their unit's queue. The mode's Mode stage spawns the trains whose time ended.
    pub fn install(_: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        schedule.add_systems(start_trains.in_set(SimSet::Act).after(CombatSet::Attack));
        registry.register_component::<TrainQueue>();
    }

    /// Spawns each train whose time ended this tick, at its unit's position, of its unit's team
    /// and player, by its unit's stable id, then its queue's order; the next in a queue starts in
    /// the same tick. A dead unit's queue waits.
    pub(crate) fn finish_trains(
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
        let ctx = world.non_send::<Ctx>().clone();
        let mode = ctx.mode().expect("a match with production has a mode");
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
                mode.spawn_owned(world, at, owner);
                let queue = world.get::<TrainQueue>(entity).expect("a producer");
                let next = queue.entries().get(1).copied();
                let next = next.map(|next| Production::time(world.resource::<ActionBook>(), next));
                let mut queue = world.get_mut::<TrainQueue>(entity).expect("a producer");
                queue.pop(now, next);
            }
        }
    }

    /// The train `slots` hold ordered, not yet checked.
    fn ordered(slots: &ActionSlots) -> Option<InProgress> {
        slots
            .in_progress()
            .filter(|underway| underway.kind == ActionKind::Train)
    }

    /// The time `queued` takes, at its rank.
    fn time(book: &ActionBook, queued: Queued) -> Ticks {
        book.get(queued.action)
            .expect("a queued train is in the book")
            .values(queued.rank)
            .windup
    }
}

/// Starts each ordered train, in Act: one that passes the core's checks, and finds a place in
/// its unit's queue, pays its whole cost, in pools and player resources, goes on cooldown, and
/// joins the queue. The order ends either way, and the unit stays free.
fn start_trains(
    tick: Res<'_, SimTick>,
    book: Res<'_, ActionBook>,
    mut resources: Option<ResMut<'_, PlayerResources>>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
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
    let trains = units.iter().filter_map(|(entity, &id, slots, ..)| {
        Production::ordered(slots).map(|_| Keyed { id, entity })
    });
    for &Keyed { entity, .. } in order.sort(trains) {
        let (_, _, mut slots, mut queue, mut pools, owner) =
            units.get_mut(entity).expect("a unit in the order");
        let ordered = Production::ordered(&slots).expect("an ordered train");
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
        if !queue.has_room() {
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
        };
        slots.cool_down(ordered.slot, now.after(values.cooldown));
        queue.push(queued, now, values.windup);
    }
}
