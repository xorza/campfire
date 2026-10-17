use std::mem;
use std::ops::{ControlFlow, Range};

use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Res, ResMut};
use bevy_ecs::world::World;
use campfire_combat::Kept;
use campfire_common::PlayerSlot;
use campfire_progression::{LevelUp, LevelUps};
use campfire_script::{Hook, PendingCalls, Pool, ScriptList, ScriptText, ScriptValue};
use campfire_sim::{EntityIndex, SimTick, SlotEvent, SlotEventKind, TickInputs};
use campfire_units::Deaths;

use crate::calls::Calls;
use crate::match_mode::MatchMode;
use crate::mode_input::{InputValue, ModeInput};
use crate::timers::Timers;
use crate::unanswered_deaths::UnansweredDeaths;
use crate::unanswered_slot_events::UnansweredSlotEvents;

/// One mode input of the tick: its player's slot, and its body in the scratch buffer.
#[derive(Debug)]
pub(super) struct Input {
    slot: PlayerSlot,
    body: Range<usize>,
}

/// The mode's hooks, each in the Mode stage: its inputs, its slot events, its timers, the deaths
/// and the level-ups. Each hook's system installs only in a mode whose script defines its hook,
/// which the load fixes.
#[derive(Debug)]
pub struct ModeHooks;

impl ModeHooks {
    /// Runs `on_mode_input` for each mode input of the tick, in input order, from its player's
    /// pool. An input whose name the mode does not declare, or whose value is not of its type, is
    /// ignored.
    pub(super) fn mode_inputs(
        world: &mut World,
        mut bodies: Local<'_, Vec<u8>>,
        mut inputs: Local<'_, Vec<Input>>,
    ) {
        bodies.clear();
        inputs.clear();
        for command in world
            .resource::<TickInputs>()
            .commands(ModeInput::CAPABILITY)
        {
            let start = bodies.len();
            bodies.extend_from_slice(command.body);
            inputs.push(Input {
                slot: command.slot,
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
                let args = (call.ctx.clone(), i64::from(input.slot.get()), name, value);
                // An input is answered once: one whose call finds the pool spent fails.
                let pool = Pool::Player(input.slot);
                if let ControlFlow::Break(error) = call.answer(None, pool, Hook::OnModeInput, args)
                {
                    call.batch.record(None, Hook::OnModeInput, &error);
                }
            }
        });
    }

    /// Runs `on_player_join` for each player who took a slot in this tick, and `on_player_leave`
    /// for each who left one, after those that wait from earlier ticks, in the order the log took
    /// them, from the mode pool. One whose call finds the pool spent waits, with those after it,
    /// for a later tick.
    pub(super) fn slot_events(world: &mut World, mut pending: Local<'_, PendingCalls<SlotEvent>>) {
        // A write marks the state changed, and the swap below is one, so a tick with no event to
        // answer leaves it as it is.
        let none_new = world.resource::<TickInputs>().slot_events().is_empty();
        if none_new && world.resource::<UnansweredSlotEvents>().0.is_empty() {
            return;
        }
        debug_assert!(pending.is_empty(), "the events go back after each run");
        mem::swap(
            &mut *pending,
            &mut world.resource_mut::<UnansweredSlotEvents>().0,
        );
        pending.extend(world.resource::<TickInputs>().slot_events().iter().copied());
        let hooks = world.resource::<MatchMode>().schema.hooks;
        let now = world.resource::<SimTick>().end();
        let answered = Calls::scope(world, now, |call| {
            let mut answered = 0;
            for event in &pending {
                let hook = match event.kind {
                    SlotEventKind::Joined => Hook::OnPlayerJoin,
                    SlotEventKind::Left => Hook::OnPlayerLeave,
                };
                if hooks.contains(hook) {
                    let args = (call.ctx.clone(), i64::from(event.slot.get()));
                    if call.answer(None, Pool::Mode, hook, args).is_break() {
                        break;
                    }
                }
                answered += 1;
            }
            answered
        });
        pending.answered(answered);
        mem::swap(
            &mut *pending,
            &mut world.resource_mut::<UnansweredSlotEvents>().0,
        );
    }

    /// Runs `on_timer` for each timer due in this tick's Mode stage, earliest first, from the mode
    /// pool. A timer whose call finds the pool spent stays due, and fires in a later tick.
    pub(super) fn run_timers(world: &mut World) {
        let now = world.resource::<SimTick>().end();
        if world.resource::<Timers>().due(now).is_none() {
            return;
        }
        Calls::scope(world, now, |call| {
            while let Some(timer) = call.batch.world().resource::<Timers>().due(now) {
                let name = ScriptText::from(timer.name.as_str());
                let data = timer
                    .data
                    .as_ref()
                    .map_or(ScriptValue::UNIT, |data| data.to_value(call.ctx.view()));
                let args = (call.ctx.clone(), name, data);
                if call
                    .answer(None, Pool::Mode, Hook::OnTimer, args)
                    .is_break()
                {
                    break;
                }
                call.batch.world().resource_mut::<Timers>().fire();
            }
        });
    }

    /// Fires each timer due in this tick's Mode stage, earliest first, in a mode that defines no
    /// `on_timer`, so no call answers it.
    pub(super) fn fire_timers(mut timers: ResMut<'_, Timers>, tick: Res<'_, SimTick>) {
        let now = tick.end();
        // A write marks the timers changed, so a tick with none due writes nothing.
        while timers.due(now).is_some() {
            timers.fire();
        }
    }

    /// Runs `on_unit_died` for each death of the tick, after those that wait from earlier ticks, in
    /// the order they happened, from the mode pool: with the unit, its killer or `()`, and its
    /// assisters; a killer gone by then is `()`, and an assister gone by then is left out. A death
    /// whose call finds the pool spent waits, with those after it, for a later tick, and its unit
    /// stays, dead, until then.
    pub(super) fn unit_deaths(
        world: &mut World,
        (mut units, mut pending): (Local<'_, Vec<Option<Entity>>>, Local<'_, UnansweredDeaths>),
    ) {
        // A write marks the state changed, and the swap below is one, so a tick with no death to
        // answer leaves it as it is.
        if world.resource::<Deaths>().is_empty() && world.resource::<UnansweredDeaths>().is_empty()
        {
            return;
        }
        let now = world.resource::<SimTick>().end();
        debug_assert!(pending.is_empty(), "the deaths go back after each run");
        mem::swap(
            &mut *pending,
            &mut *world.resource_mut::<UnansweredDeaths>(),
        );
        pending.extend(world.resource::<Deaths>().iter());
        let answered = Calls::scope(world, now, |call| {
            let view = call.ctx.view().clone();
            let mut answered = 0;
            for death in pending.iter() {
                if let Some(unit) = view.unit(death.id) {
                    let killer = death.killer.and_then(|id| view.unit(id));
                    let killer = killer.map_or(ScriptValue::UNIT, ScriptValue::of);
                    let assisters = death.assisters.iter().filter_map(|&id| view.unit(id));
                    let assisters: ScriptList = assisters.map(ScriptValue::of).collect();
                    let args = (call.ctx.clone(), ScriptValue::of(unit), killer, assisters);
                    let unit = Some(death.id);
                    if call
                        .answer(unit, Pool::Mode, Hook::OnUnitDied, args)
                        .is_break()
                    {
                        break;
                    }
                }
                answered += 1;
            }
            answered
        });
        let index = world.resource::<EntityIndex>();
        units.clear();
        units.extend(pending.iter().map(|death| index.get(death.id)));
        for (at, entity) in units.iter().enumerate() {
            let Some(entity) = *entity else {
                continue;
            };
            let mut unit = world.entity_mut(entity);
            if at < answered {
                unit.remove::<Kept>();
            } else {
                unit.insert(Kept);
            }
        }
        pending.answered(answered);
        mem::swap(
            &mut *pending,
            &mut *world.resource_mut::<UnansweredDeaths>(),
        );
    }

    /// Runs `on_level_up` for each level a unit reached, in the order reached, from the mode pool:
    /// with the unit, the track's name and the level. A level that a call reaches joins the end, so
    /// a chain of level-ups ends within the tick, as levels are finite. A level-up whose call finds
    /// the pool spent waits, with those after it, for a later tick; one whose unit is gone by then
    /// runs no call.
    pub(super) fn level_ups(world: &mut World, mut due: Local<'_, PendingCalls<LevelUp>>) {
        let now = world.resource::<SimTick>().end();
        loop {
            if world.resource::<LevelUps>().0.is_empty() {
                return;
            }
            due.clear();
            mem::swap(&mut *due, &mut world.resource_mut::<LevelUps>().0);
            let answered = Calls::scope(world, now, |call| {
                let view = call.ctx.view().clone();
                let mut answered = 0;
                for &LevelUp { id, track, level } in &due {
                    if let Some(handle) = view.unit(id) {
                        let level = i64::from(level.get());
                        let name = view.names().name(track);
                        let args = (call.ctx.clone(), handle, name, level);
                        if call
                            .answer(Some(id), Pool::Mode, Hook::OnLevelUp, args)
                            .is_break()
                        {
                            break;
                        }
                    }
                    answered += 1;
                }
                answered
            });
            if answered < due.len() {
                let mut level_ups = world.resource_mut::<LevelUps>();
                due.answered(answered);
                due.append(&mut level_ups.0);
                mem::swap(&mut *due, &mut level_ups.0);
                return;
            }
        }
    }

    /// Clears the levels reached, in a mode that defines no `on_level_up`, so no call answers
    /// them.
    pub(super) fn clear_level_ups(mut level_ups: ResMut<'_, LevelUps>) {
        // A write marks the state changed, so a tick with no level-up writes nothing.
        if !level_ups.0.is_empty() {
            level_ups.0.clear();
        }
    }
}
