use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionCall, ActionSlots, ChannelCall, ChannelStep, UseGroup};
use campfire_common::{Tick, Ticks};
use campfire_script::{CallStart, Hook, Pool, ScriptBatch, ScriptsWorldCrate};
use campfire_sim::{Keyed, Ordered, SimTick, StableId};
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
    /// the units, read as the batch begins.
    pub(super) fn run_channels(
        world: &mut World,
        units: &mut QueryState<(Entity, &StableId, &ActionSlots)>,
        (mut order, mut due): (Local<'_, Ordered>, Local<'_, Vec<Keyed>>),
    ) {
        let now = world.resource::<SimTick>().start();
        let running = units
            .iter(world)
            .filter(|(.., slots)| slots.channel_due())
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
    }

    /// Runs the channel of `entity` at `now`, as `run_channels` does, and gives back the hooks due.
    fn step_channel(world: &mut World, now: Tick, entity: Entity) -> ChannelStep {
        let unit = world.entity(entity);
        let slots = unit
            .get::<ActionSlots>()
            .expect("a unit whose channel runs has slots");
        let forced = unit.contains::<ForcedMove>();
        let blocked = Self::blocked(slots, unit.get(), forced, unit.get());
        let tick = Self::channel_tick(world.resource::<ActionBook>(), slots);
        world
            .get_mut::<ActionSlots>(entity)
            .expect("a unit whose channel runs has slots")
            .step_channel(now, blocked, tick)
    }

    /// Runs `hook` of the channel `call` of `caster`, the unit of `entity`, of its action at its
    /// rank, as the action started, with the unit and, for `on_interrupt`, the target, when the
    /// action's script defines it; a failed call applies nothing, and is recorded.
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
        batch.hook_call(start, hook, Some(caster), |batch| match hook {
            Hook::OnInterrupt => {
                let target = Abilities::target(ctx.view(), aim.target);
                batch.call_hook(pool, script, hook, (ctx.clone(), handle, target))
            }
            _ => batch.call_hook(pool, script, hook, (ctx.clone(), handle)),
        });
    }

    /// Runs each channel of a unit a client predicts as the server does, with no call: a cut one's
    /// record clears, and one at its end ends, as the hooks' effects come from the server.
    pub(super) fn predict_channels(
        tick: Res<'_, SimTick>,
        book: Res<'_, ActionBook>,
        mut units: Query<
            '_,
            '_,
            (
                &mut ActionSlots,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&UseGroup>,
            ),
        >,
    ) {
        let now = tick.start();
        for (mut slots, tags, forced, used) in &mut units {
            if !slots.channel_due() {
                continue;
            }
            let blocked = Self::blocked(&slots, tags, forced, used);
            let tick = Self::channel_tick(&book, &slots);
            slots.step_channel(now, blocked, tick);
        }
    }

    /// Whether the tags of a unit that holds `slots` keep its channel from running, as its slot's
    /// kind says, under a forced move when `forced`, and of the use group `used`.
    fn blocked(
        slots: &ActionSlots,
        tags: Option<&UnitTags>,
        forced: bool,
        used: Option<&UseGroup>,
    ) -> bool {
        let channel = slots.channeling().and_then(|slot| slots.slot(slot));
        let kind = channel.map(|slot| slot.kind);
        Abilities::blocked(tags, forced, used, kind)
    }

    /// The time between the ticks of the channel `slots` runs, if one runs.
    fn channel_tick(book: &ActionBook, slots: &ActionSlots) -> Ticks {
        slots
            .channeling()
            .and_then(|slot| slots.slot(slot))
            .and_then(|slot| slot.values_in(book)?.channel)
            .map_or(Ticks::ZERO, |rule| rule.tick)
    }
}
