use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionCall, ActionProgress, ActionSlot, ActionSlots, Caster, ChannelCall,
    ChannelStep, UseGroup,
};
use campfire_common::{Tick, Ticks};
use campfire_script::{CallStart, Hook, Pool, ScriptBatch, ScriptsWorld};
use campfire_sim::{Keyed, Ordered, SimTick, StableId};
use campfire_stats::{Held, HeldModifiers};
use campfire_units::{ForcedMove, Owner};
use campfire_values::UnitTags;

use crate::Abilities;

/// The channels units run, in Hit: their hooks on the server, and on a client their cut and their
/// end alone.
#[derive(Debug)]
pub struct Channels;

impl Channels {
    /// Runs each unit's channel in Hit, in the order of its stable id, after the casts resolve: a
    /// cut one's `on_interrupt`, then a due tick's `on_channel_tick`, each a call of the channel's
    /// action at its rank from the unit, in its player's pool; and one at its end ends. A unit
    /// whose tags keep it from casting has its channel cut. A channel's calls share one snapshot of
    /// the units, read as the batch begins. A channel that still runs holds its action's `hold`
    /// on its unit this tick, from the unit at its rank.
    pub(super) fn run_channels(
        world: &mut World,
        units: &mut QueryState<(Entity, &StableId, &ActionProgress)>,
        (mut order, mut due): (Local<'_, Ordered>, Local<'_, Vec<Keyed>>),
    ) {
        let now = world.resource::<SimTick>().start();
        let running = units
            .iter(world)
            .filter(|(.., progress)| progress.channel_due())
            .map(|(entity, &id, _)| Keyed { id, entity });
        due.clear();
        due.extend_from_slice(order.sort(running));
        if due.is_empty() {
            return;
        }
        world.script_batch(|batch| {
            for &Keyed { id, entity } in &*due {
                let step = Self::step_channel(batch.world(), now, entity);
                for (call, hook) in [
                    (step.interrupted, Hook::OnInterrupt),
                    (step.ticked, Hook::OnChannelTick),
                ] {
                    if let Some(call) = call {
                        Self::channel_call(batch, now, id, entity, call, hook);
                    }
                }
            }
        });
        for &Keyed { id, entity } in &*due {
            let progress = world.get::<ActionProgress>(entity);
            let held = progress.and_then(|progress| {
                Self::channel_hold(world.resource::<ActionBook>(), progress, id)
            });
            if let Some(held) = held {
                world.resource_mut::<HeldModifiers>().0.push(held);
            }
        }
    }

    /// The `hold` the channel `progress` runs holds on its unit `id`, from the unit at its rank;
    /// none with no channel running or no hold.
    fn channel_hold(book: &ActionBook, progress: &ActionProgress, id: StableId) -> Option<Held> {
        let channel = progress.in_progress()?.channel_call()?;
        let hold = book.get(channel.action)?.hold?;
        Some(Held {
            target: id,
            modifier: hold,
            source: Some(id),
            action: Some(channel.action),
            rank: channel.rank,
        })
    }

    /// Runs the channel of `entity` at `now`, as `run_channels` does, and gives back the hooks due.
    fn step_channel(world: &mut World, now: Tick, entity: Entity) -> ChannelStep {
        let unit = world.entity(entity);
        let slots = unit
            .get::<ActionSlots>()
            .expect("a unit whose channel runs has slots");
        let progress = unit
            .get::<ActionProgress>()
            .expect("a unit whose channel runs has progress");
        let forced = unit.contains::<ForcedMove>();
        let book = world.resource::<ActionBook>();
        let channel = Self::channel(slots, progress);
        let blocked = Caster::of(unit.get(), forced, unit.get()).blocked(channel, book);
        let tick = Self::channel_tick(book, channel);
        let mut progress = world
            .get_mut::<ActionProgress>(entity)
            .expect("a unit whose channel runs has progress");
        if blocked {
            progress.cut_channel();
        }
        progress.step_channel(now, tick)
    }

    /// Runs `hook` of the channel `call` of `caster`, the unit of `entity`, of its action at its
    /// rank, as the action started, with the unit and the target, when the action's script defines
    /// it; a failed call applies nothing, and is recorded.
    fn channel_call(
        batch: &mut ScriptBatch<'_>,
        now: Tick,
        caster: StableId,
        entity: Entity,
        call: ChannelCall,
        hook: Hook,
    ) {
        let ctx = &batch.ctx().clone();
        let ChannelCall {
            call: ActionCall { aim, start },
            action: id,
            rank,
        } = call;
        let world = batch.world();
        let owner = world.get::<Owner>(entity).map(|owner| owner.slot());
        let book = world.resource::<ActionBook>();
        let action = book.get(id).expect("a channel's action is in the book");
        let (Some(script), Some(handle)) = (action.hook(hook), ctx.view().unit(caster)) else {
            return;
        };
        let package = action.package;
        let start = CallStart {
            start: Some(start),
            ..CallStart::cast(id, rank, caster, package, now)
        };
        let pool = Pool::of(owner);
        let target = Abilities::target(ctx.view(), aim.target);
        batch.hook_call(start, hook, Some(caster), |batch| {
            batch
                .call(pool, script, hook, (ctx.clone(), handle, target))
                .map(drop)
        });
    }

    /// Runs each channel of a unit a client predicts as the server does, with no call: a cut one's
    /// record clears, and one at its end ends, as the hooks' effects come from the server; one
    /// that still runs holds its `hold`, as on the server.
    pub(super) fn predict_channels(
        tick: Res<'_, SimTick>,
        book: Res<'_, ActionBook>,
        mut held: ResMut<'_, HeldModifiers>,
        mut units: Query<
            '_,
            '_,
            (
                &StableId,
                &ActionSlots,
                &mut ActionProgress,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&UseGroup>,
            ),
        >,
    ) {
        let now = tick.start();
        for (&id, slots, mut progress, tags, forced, used) in &mut units {
            if !progress.channel_due() {
                continue;
            }
            let channel = Self::channel(slots, &progress);
            let blocked = Caster::of(tags, forced, used).blocked(channel, &book);
            let tick = Self::channel_tick(&book, channel);
            if blocked {
                progress.cut_channel();
            }
            progress.step_channel(now, tick);
            held.0.extend(Self::channel_hold(&book, &progress, id));
        }
    }

    /// The slot of `slots` whose channel `progress` runs, if one runs.
    fn channel(slots: &ActionSlots, progress: &ActionProgress) -> Option<ActionSlot> {
        slots.slot(progress.channeling()?)
    }

    /// The time between the ticks of the channel of `channel`, if one runs.
    fn channel_tick(book: &ActionBook, channel: Option<ActionSlot>) -> Ticks {
        channel
            .and_then(|slot| slot.values_in(book)?.channel)
            .map_or(Ticks::ZERO, |rule| rule.tick)
    }
}
