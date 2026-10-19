use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionCall, ActionProgress, ActionSlot, ActionSlots, Caster, ChannelCall,
    ChannelStep, ProgressDues, ProgressIndexes, UseGroup,
};
use campfire_common::{Tick, Ticks};
use campfire_script::{
    CallStart, Ctx, Hook, HookDecl, ScriptBatch, ScriptRole, ScriptValue, ScriptsWorld, Unit,
};
use campfire_sim::{DueUnit, Keyed, Ordered, SimTick, StableId};
use campfire_stats::{Held, HeldModifiers};
use campfire_units::{Due, ForcedMove, Owner};
use campfire_values::{Motion, UnitTags};

use crate::Abilities;

/// The channels units run, in Hit: their hooks on the server, and on a client their cut and their
/// end alone.
/// A unit whose channel is due, keyed by its stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelDue(Keyed);

#[derive(Debug)]
pub struct Channels;

/// `on_channel_tick(ctx, caster, target)`: a channel's tick, at the target it aims at.
#[derive(Debug)]
pub(crate) struct OnChannelTick;

/// `on_interrupt(ctx, caster, target)`: a channel cut before its end.
#[derive(Debug)]
pub(crate) struct OnInterrupt;

impl HookDecl for OnChannelTick {
    const NAME: &'static str = "on_channel_tick";
    const ROLE: ScriptRole = ScriptRole::Action;
    const PARAMS: &'static [&'static str] = &["ctx", "caster", "target"];
    type Args = (Ctx, Unit, ScriptValue);
}

impl HookDecl for OnInterrupt {
    const NAME: &'static str = "on_interrupt";
    const ROLE: ScriptRole = ScriptRole::Action;
    const PARAMS: &'static [&'static str] = &["ctx", "caster", "target"];
    type Args = (Ctx, Unit, ScriptValue);
}

impl Channels {
    /// Runs each unit's channel in Hit, in the order of its stable id, after the casts resolve: a
    /// cut one's `on_interrupt`, then a due tick's `on_channel_tick`, each a call of the channel's
    /// action at its rank from the unit, in its player's pool; and one at its end ends. A unit
    /// whose tags keep it from casting has its channel cut. A channel's calls share one snapshot of
    /// the units, read as the batch begins. A channel that still runs holds its action's `hold`
    /// on its unit this tick, from the unit at its rank.
    pub(super) fn run_channels(world: &mut World) {
        let due = world.resource_mut::<Due<ChannelDue>>().take();
        if !due.is_empty() {
            Self::run(world, &due);
        }
        world.resource_mut::<Due<ChannelDue>>().give_back(due);
    }

    /// Collects the units whose channel is due, by stable id, for `run_channels`: those the
    /// index of the channels holds, which it forgets once their channel ends.
    pub(super) fn collect_channels(
        units: Query<'_, '_, &ActionProgress>,
        mut indexes: ProgressIndexes<'_>,
        (tick, mut order, mut held): (
            Res<'_, SimTick>,
            Local<'_, Ordered>,
            Local<'_, Vec<DueUnit>>,
        ),
        mut due: ResMut<'_, Due<ChannelDue>>,
    ) {
        held.clear();
        held.extend(indexes.channels(tick.start()));
        held.retain(|unit| {
            let runs = units
                .get(unit.entity)
                .is_ok_and(ActionProgress::is_channel_due);
            if !runs {
                indexes.forget_channel(unit.id);
            }
            runs
        });
        let running = held
            .iter()
            .map(|&DueUnit { id, entity }| Keyed { id, entity });
        due.refill(order.sort(running).iter().map(|&keyed| ChannelDue(keyed)));
    }

    /// Runs the channels of `due`, in order.
    fn run(world: &mut World, due: &[ChannelDue]) {
        let now = world.resource::<SimTick>().start();
        world.script_batch(|batch| {
            for &ChannelDue(Keyed { id, entity }) in due {
                let step = Self::step_channel(batch.world(), now, id, entity);
                if let Some(call) = step.interrupted {
                    Self::channel_call::<OnInterrupt>(batch, now, id, entity, call);
                }
                if let Some(call) = step.ticked {
                    Self::channel_call::<OnChannelTick>(batch, now, id, entity, call);
                }
            }
        });
        for &ChannelDue(Keyed { id, entity }) in due {
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
    fn step_channel(world: &mut World, now: Tick, id: StableId, entity: Entity) -> ChannelStep {
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
        let blocked = Caster::of(unit.get(), Motion::of(forced), unit.get()).blocked(channel, book);
        let tick = Self::channel_tick(book, channel);
        let step = ProgressDues::change(world, id, entity, |progress, dues| {
            if blocked {
                progress.cut_channel(dues);
            }
            progress.step_channel(now, tick, dues)
        });
        step.expect("a unit whose channel runs has progress")
    }

    /// Runs the hook `H` of the channel `call` of `caster`, the unit of `entity`, of its action at
    /// its rank, as the action started, with the unit and the target, when the action's script
    /// defines it; a failed call applies nothing, and is recorded.
    fn channel_call<H: HookDecl<Args = (Ctx, Unit, ScriptValue)>>(
        batch: &mut ScriptBatch<'_>,
        now: Tick,
        caster: StableId,
        entity: Entity,
        call: ChannelCall,
    ) {
        let hook = Hook::of::<H>();
        let ctx = &batch.ctx().clone();
        let ChannelCall {
            call: ActionCall { aim, start },
            action: id,
            rank,
        } = call;
        let world = batch.world();
        let owner = world.get::<Owner>(entity).map(|owner| owner.seat());
        let book = world.resource::<ActionBook>();
        let action = book.get(id).expect("a channel's action is in the book");
        let (Some(script), Some(handle)) = (action.hook(hook), ctx.view().unit(caster)) else {
            return;
        };
        let package = action.package;
        let start = CallStart {
            start: Some(start),
            payer: owner,
            ..CallStart::cast(id, rank, caster, package, now)
        };
        let target = Abilities::target(ctx.view(), aim.target);
        batch.answer(start, hook, Some(caster), |batch| {
            let args = (ctx.clone(), handle, target);
            batch.call::<H>(script, args).map(drop)
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
                &ActionSlots,
                &mut ActionProgress,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&UseGroup>,
            ),
        >,
        mut indexes: ProgressIndexes<'_>,
        mut due: Local<'_, Vec<DueUnit>>,
    ) {
        let now = tick.start();
        due.clear();
        due.extend(indexes.channels(now));
        for &DueUnit { id, entity } in &*due {
            let Ok((slots, mut progress, tags, forced, used)) = units.get_mut(entity) else {
                indexes.forget_channel(id);
                continue;
            };
            if !progress.is_channel_due() {
                indexes.forget_channel(id);
                continue;
            }
            let dues = &mut indexes.of(id, entity);
            let channel = Self::channel(slots, &progress);
            let blocked = Caster::of(tags, Motion::of(forced), used).blocked(channel, &book);
            let tick = Self::channel_tick(&book, channel);
            if blocked {
                progress.cut_channel(dues);
            }
            progress.step_channel(now, tick, dues);
            if !progress.is_channel_due() {
                indexes.forget_channel(id);
            }
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
