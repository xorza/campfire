use std::mem;
use std::ops::Range;

use bevy_ecs::system::{Local, Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::{
    Ctx, HookDecl, IdName, ModeEvents, PlayerHandle, ScriptList, ScriptRole, ScriptText,
    ScriptValue,
};
use campfire_sim::{SimTick, TickInputs, UnreadCause, UnreadCommands};

use crate::calls::Calls;
use crate::mode_input::{InputValue, ModeInput};
use crate::timer_id::TimerId;
use crate::timers::Timers;

/// One mode input of the tick: its player's player slot, and its body in the scratch buffer.
#[derive(Debug)]
pub(super) struct Input {
    player_slot: PlayerSlot,
    body: Range<usize>,
}

/// The mode's hooks, each in the Mode stage: its inputs, its slot events, its timers, the deaths
/// and the level-ups. Each hook's system installs only in a mode one of whose scripts defines its
/// hook, which the load fixes.
#[derive(Debug)]
pub(crate) struct ModeHooks;

/// `on_match_start(ctx)`: the match starts, its units spawned.
#[derive(Debug)]
pub(crate) struct OnMatchStart;

/// `on_input(ctx, player, name, value)`: a player's mode input.
#[derive(Debug)]
pub(crate) struct OnInput;

/// `on_player_join(ctx, player)`: a player takes a slot.
#[derive(Debug)]
pub(crate) struct OnPlayerJoin;

/// `on_player_leave(ctx, player)`: a player leaves a slot.
#[derive(Debug)]
pub(crate) struct OnPlayerLeave;

/// `on_timer(ctx, name, data)`: a timer of the mode's is due.
#[derive(Debug)]
pub(crate) struct OnTimer;

/// `on_generate(ctx, region)`: planned, the map's generation fills a region.
#[derive(Debug)]
pub(crate) struct OnGenerate;

/// `on_system(ctx, units)`: planned, a script system runs over its units.
#[derive(Debug)]
pub(crate) struct OnSystem;

/// `on_quest_stage(ctx, player, quest, stage)`: planned, a player ends a stage of a quest.
#[derive(Debug)]
pub(crate) struct OnQuestStage;

/// A hook of the mode's script.
macro_rules! mode_hook {
    ($($hook:ident: $name:literal, [$($param:literal),*], $args:ty;)*) => {$(
        impl HookDecl for $hook {
            const NAME: &'static str = $name;
            const ROLE: ScriptRole = ScriptRole::Mode;
            const PARAMS: &'static [&'static str] = &["ctx", $($param),*];
            type Args = $args;
        }
    )*};
}

mode_hook! {
    OnMatchStart: "on_match_start", [], (Ctx,);
    OnInput: "on_input", ["player", "name", "value"], (Ctx, PlayerHandle, ScriptText, ScriptValue);
    OnPlayerJoin: "on_player_join", ["player"], (Ctx, PlayerHandle);
    OnPlayerLeave: "on_player_leave", ["player"], (Ctx, PlayerHandle);
    OnTimer: "on_timer", ["name", "data"], (Ctx, IdName<TimerId>, ScriptValue);
    OnGenerate: "on_generate", ["region"], (Ctx, ScriptValue);
    OnSystem: "on_system", ["units"], (Ctx, ScriptList);
    OnQuestStage: "on_quest_stage", ["player", "quest", "stage"], (Ctx, PlayerHandle, ScriptText, i64);
}

impl ModeHooks {
    /// Runs `on_input` for each mode input of the tick, in input order, from its player's
    /// pool. An input whose name the mode does not declare, or whose value is not of its type, is
    /// none the mode reads, which the tick's report of unread commands keeps (02-engine-core.md,
    /// A command nothing reads is counted).
    pub(super) fn mode_inputs(
        world: &mut World,
        mut bodies: Local<'_, Vec<u8>>,
        mut inputs: Local<'_, Vec<Input>>,
        mut undecoded: Local<'_, Vec<PlayerSlot>>,
    ) {
        bodies.clear();
        inputs.clear();
        undecoded.clear();
        for command in world
            .resource::<TickInputs>()
            .commands(ModeInput::ADDRESSEE)
        {
            let start = bodies.len();
            bodies.extend_from_slice(command.body);
            inputs.push(Input {
                player_slot: command.player_slot,
                body: start..bodies.len(),
            });
        }
        if inputs.is_empty() {
            return;
        }
        let now = world.resource::<SimTick>().start();
        Calls::scope(world, now, |call| {
            for input in &*inputs {
                let Some(decoded) = ModeInput::decode(&bodies[input.body.clone()], |name| {
                    call.book().schema.input_type_named(name)
                }) else {
                    undecoded.push(input.player_slot);
                    continue;
                };
                let value = match decoded.value {
                    InputValue::String(text) => ScriptValue::of(ScriptText::from(text)),
                    InputValue::StringList(texts) => ScriptValue::of::<ScriptList>(
                        texts
                            .into_iter()
                            .map(|text| ScriptValue::of(ScriptText::from(text)))
                            .collect(),
                    ),
                };
                let name = ScriptText::from(decoded.name);
                let args = (
                    call.ctx.clone(),
                    PlayerHandle::new(input.player_slot),
                    name,
                    value,
                );
                // An input is answered once: one whose call finds its player's pool spent fails.
                let answered = call.answer::<OnInput>(None, Some(input.player_slot), args);
                debug_assert!(answered.is_continue(), "an input's call never waits");
            }
        });
        let mut unread = world.resource_mut::<UnreadCommands>();
        for &player_slot in &*undecoded {
            unread.push(player_slot, UnreadCause::Undecodable(ModeInput::ADDRESSEE));
        }
    }

    /// Answers the events of the queue `Q` in this tick's Mode stage, those that wait from
    /// earlier ticks first, then the tick's, in order, through the mode's scripts. An event whose
    /// call finds the mode's pool spent waits, with those after it, for a later tick, which runs
    /// it in the scripts that did not answer it; one that a call adds joins the end, and runs
    /// within the tick when every event before it ran.
    pub(super) fn answer<Q: ModeEvents>(world: &mut World, mut pending: Local<'_, Q>) {
        // A write marks the state changed, so a tick with no event to answer writes nothing.
        Q::arrive(world);
        let now = world.resource::<SimTick>().end();
        loop {
            if world.resource::<Q>().is_empty() {
                return;
            }
            debug_assert!(pending.is_empty(), "the events go back after each run");
            mem::swap(&mut *pending, &mut *world.resource_mut::<Q>());
            let resume = pending.partial();
            let (answered, partial) = Calls::scope(world, now, |call| {
                let mut answer = call.mode_answer(resume);
                let answered = pending.answer(&mut answer);
                (answered, answer.partial())
            });
            pending.hold(answered, world);
            let all = answered == pending.len();
            pending.answered(answered, partial);
            let mut queue = world.resource_mut::<Q>();
            pending.append(&mut queue);
            mem::swap(&mut *pending, &mut *queue);
            if !all {
                return;
            }
        }
    }

    /// Runs `on_timer` of the script that set each timer due in this tick's Mode stage, earliest
    /// first, from the mode pool, or fires it with no call when that script defines none. A timer
    /// whose call finds the pool spent stays due, and fires in a later tick.
    pub(super) fn run_timers(world: &mut World) {
        let now = world.resource::<SimTick>().end();
        if world.resource::<Timers>().due(now).is_none() {
            return;
        }
        Calls::scope(world, now, |call| {
            while let Some(timer) = call.batch.world().resource::<Timers>().due(now) {
                let name = ScriptText::from(call.ctx.view().names().name(timer.name));
                let data = timer
                    .data
                    .as_ref()
                    .map_or(ScriptValue::UNIT, |data| data.to_value(call.ctx.view()));
                let script = timer.script;
                let args = |ctx: &Ctx| (ctx.clone(), IdName::new(name.clone()), data.clone());
                if call.answer_in::<OnTimer>(script, args).is_break() {
                    break;
                }
                call.batch.world().resource_mut::<Timers>().fire();
            }
        });
    }

    /// Fires each timer due in this tick's Mode stage, earliest first, in a mode none of whose
    /// scripts defines `on_timer`, so no call answers it.
    pub(super) fn fire_timers(mut timers: ResMut<'_, Timers>, tick: Res<'_, SimTick>) {
        let now = tick.end();
        // A write marks the timers changed, so a tick with none due writes nothing.
        while timers.due(now).is_some() {
            timers.fire();
        }
    }

    /// Drops the events of the queue `Q`, in a mode none of whose scripts defines one of its
    /// hooks, so no call answers them.
    pub(super) fn drop<Q: ModeEvents>(mut queue: ResMut<'_, Q>) {
        // A write marks the state changed, so a tick with no event writes nothing.
        if !queue.is_empty() {
            let len = queue.len();
            queue.answered(len, 0);
        }
    }
}
