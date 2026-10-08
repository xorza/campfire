use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryState;
use bevy_ecs::system::Local;
use bevy_ecs::world::{Mut, World};
use campfire_common::{Tick, Ticks};
use campfire_sim::{Keyed, Ordered, SimTick, StableId};

use crate::abilities::Abilities;
use crate::actions::action_book::ActionBook;
use crate::actions::action_call::ActionCall;
use crate::actions::action_slots::ActionSlots;
use crate::actions::channel_call::{ChannelCall, ChannelStep};
use crate::items::inventory::Inventory;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::units::forced_move::ForcedMove;
use crate::units::owner::Owner;
use crate::units::unit_tags::UnitTags;

/// The channels units run, in Hit: their hooks on the server, and on a client their cut and their
/// end alone.
#[derive(Debug)]
pub(super) struct Channels;

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
        let ctx = world.non_send::<Ctx>().clone();
        ScriptBatch::run(world, ctx.view(), |batch| {
            for &Keyed { id, entity } in &*due {
                let step = Self::step_channel(batch.world(), now, entity);
                for (call, hook) in [
                    (step.interrupted, Hook::OnInterrupt),
                    (step.ticked, Hook::OnChannelTick),
                ] {
                    if let Some(call) = call {
                        Self::channel_call(batch, &ctx, now, id, entity, call, hook);
                    }
                }
            }
        });
    }

    /// Runs the channel of `entity` at `now`, as `run_channels` does, and gives back the hooks due.
    fn step_channel(world: &mut World, now: Tick, entity: Entity) -> ChannelStep {
        let unit = world.entity(entity);
        let forced = unit.contains::<ForcedMove>();
        let slots = unit.get::<ActionSlots>();
        let channel = slots.and_then(|slots| slots.slot(slots.channeling()?));
        let kind = channel.map(|slot| slot.kind);
        let blocked = Abilities::blocked(
            unit.get::<UnitTags>(),
            forced,
            unit.get::<Inventory>(),
            kind,
        );
        world.resource_scope(|world, book: Mut<'_, ActionBook>| {
            let mut slots = world
                .get_mut::<ActionSlots>(entity)
                .expect("a unit whose channel runs has slots");
            let tick = Self::channel_tick(&book, &slots);
            slots.step_channel(now, blocked, tick)
        })
    }

    /// Runs `hook` of the channel `call` of `caster`, the unit of `entity`, of its action at its
    /// rank, as the action started, with the unit and, for `on_interrupt`, the target, when the
    /// action's script defines it; a failed call applies nothing, and is recorded.
    fn channel_call(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        now: Tick,
        caster: StableId,
        entity: Entity,
        call: ChannelCall,
        hook: Hook,
    ) {
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
            ..CallStart::cast(id, rank, caster, package)
        };
        let pool = Pool::of(owner);
        batch.hook_call(ctx, now, start, hook, Some(caster), |batch| match hook {
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
        world: &mut World,
        units: &mut QueryState<(Entity, &ActionSlots)>,
        mut due: Local<'_, Vec<Entity>>,
    ) {
        let now = world.resource::<SimTick>().start();
        due.clear();
        due.extend(
            units
                .iter(world)
                .filter(|(_, slots)| slots.channel_due())
                .map(|(entity, _)| entity),
        );
        for &entity in &*due {
            Self::step_channel(world, now, entity);
        }
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
