//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

use std::mem;
use std::ops::{ControlFlow, Range};
use std::rc::Rc;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::Local;
use bevy_ecs::world::{Mut, World};
use campfire_common::{PlayerSlot, Tick};
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{
    EntityIndex, IdAllocator, SimSet, SimTick, SlotEventKind, StateRegistry, TickInputs, TickRate,
};

use crate::abilities::AbilitiesSet;
use crate::actions::ActionsSet;
use crate::actions::action_slots::ActionSlots;
use crate::combat::CombatSet;
use crate::combat::assist_window::AssistWindow;
use crate::combat::damage_weigher::DamageWeigher;
use crate::combat::deaths::Deaths;
use crate::combat::heal_weigher::HealWeigher;
use crate::combat::kept::Kept;
use crate::combat::respawn::Respawn;
use crate::mode::calls::Calls;
use crate::mode::choices::Choices;
use crate::mode::game_map::GameMap;
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_call::ModeCall;
use crate::mode::mode_effect::ModeEffect;
use crate::mode::mode_input::{InputValue, ModeInput};
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_state::ModeState;
use crate::mode::placed_unit::PlacedPath;
use crate::mode::save_asked::SaveAsked;
use crate::mode::timers::Timers;
use crate::mode::unanswered_deaths::UnansweredDeaths;
use crate::mode::unanswered_slot_events::UnansweredSlotEvents;
use crate::navigation::NavigationSet;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::orders::OrdersSet;
use crate::players::player_resources::PlayerResources;
use crate::production::ProductionSet;
use crate::progression::level_ups::{LevelUp, LevelUps};
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pending_calls::PendingCalls;
use crate::scripts::pool::Pool;
use crate::scripts::script_book::ScriptBook;
use crate::stats::StatsSet;
use crate::units::UnitsSet;
use crate::units::relations::Relations;
use crate::units::script_view::View;
use crate::units::spawner::{SpawnAt, Spawner};
use crate::units::team::Team;
use crate::vision::Vision;

pub(crate) mod calls;
pub(crate) mod choice_book;
pub(crate) mod choice_data;
pub(crate) mod choices;
pub(crate) mod error;
pub(crate) mod game_map;
pub(crate) mod group_unit;
pub(crate) mod map_data;
pub(crate) mod marker;
pub(crate) mod match_end;
pub(crate) mod mode_api;
pub(crate) mod mode_book;
pub(crate) mod mode_books;
pub(crate) mod mode_call;
pub(crate) mod mode_data;
pub(crate) mod mode_effect;
pub(crate) mod mode_input;
pub(crate) mod mode_map;
pub(crate) mod mode_schema;
pub(crate) mod mode_setup;
pub(crate) mod mode_state;
pub(crate) mod mode_units;
pub(crate) mod offer;
pub(crate) mod placed_unit;
pub(crate) mod players_data;
pub(crate) mod relation_data;
pub(crate) mod roster;
pub(crate) mod save_asked;
pub(crate) mod saves_data;
pub(crate) mod team_manifest;
pub(crate) mod timers;
pub(crate) mod unanswered_deaths;
pub(crate) mod unanswered_slot_events;
pub(crate) mod unit_kit;

/// The mode of a match: the core's rules above the capabilities. Every match installs it after
/// its capabilities.
#[derive(Debug)]
pub struct Mode;

impl Mode {
    /// Adds the mode of `setup`, whose books the book builder built, to a match whose capabilities
    /// are installed and whose unit types, abilities and AI are loaded: in Inputs, the players'
    /// mode inputs run `on_mode_input`; in Mode, the trains whose time ended spawn, the tick's
    /// joins and leaves run `on_player_join` and `on_player_leave`, due timers run `on_timer`, the
    /// tick's deaths run `on_unit_died`, and the levels reached run `on_level_up`.
    /// The map's ground, paths and grid become the match's, and the mode's `[combat]` and
    /// `calc_damage` combat's.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        setup: ModeSetup<'_>,
        mut books: ModeBooks,
    ) {
        let view = world.non_send::<View>().clone();
        let rate = *world.resource::<TickRate>();
        let walkers = mem::take(&mut books.walkers);
        let loadout_ranks = books.loadout_ranks;
        // A window past what ticks can count covers the whole match.
        let assist_window = setup.data.combat.assist_window_ms.map(|ms| rate.window(ms));
        let resource_count = books.resources.len();
        let ModeMap {
            ground,
            paths,
            placed,
            markers,
            grid,
            brush,
        } = books.install(world);
        ground.install(world, walkers);
        let state = ModeState::initial(setup.data);
        let book = ModeBook::new(
            setup,
            world.resource::<ScriptBook>(),
            placed,
            GameMap::new(paths.names().map(ImmutableString::from), &markers),
            loadout_ranks,
        );
        if let Some(grid) = grid {
            Vision::load_grid(world, grid, &brush, book.teams.count());
        }
        if let Some(window) = assist_window {
            world.insert_resource(AssistWindow(window));
        }
        view.set_names(Rc::clone(&book.teams), paths.shared_names());
        world.insert_resource(paths);
        world.insert_resource(state);
        let players = book.teams.players() as usize;
        world.insert_resource(book.choices.empty(players));
        world.insert_resource(PlayerResources::new(players, resource_count));
        world.insert_resource(Timers::default());
        world.insert_resource(UnansweredDeaths::default());
        world.insert_resource(UnansweredSlotEvents::default());
        let hooks = book.schema.hooks;
        let ctx = world.non_send::<Ctx>().clone();
        let book = Rc::new(book);
        ctx.frame().add_part(ModeCall::new(Rc::clone(&book)));
        ctx.set_mode(book);
        let spawning = ctx.clone();
        world.insert_non_send(Spawner::new(move |world, at, owner| {
            let mode = ModeBook::of_match(&spawning);
            mode.spawn_owned(world, at, owner)
        }));
        if hooks.contains(Hook::CalcDamage) {
            let ctx = ctx.clone();
            world.insert_non_send(DamageWeigher::new(move |batch, damage| {
                Calls::weigh(batch, &ctx, damage)
            }));
        }
        if hooks.contains(Hook::CalcHeal) {
            world.insert_non_send(HealWeigher::new(move |batch, heal| {
                Calls::weigh_heal(batch, &ctx, heal)
            }));
        }
        schedule.add_systems((
            mode_inputs
                .in_set(SimSet::Inputs)
                .after(UnitsSet::BeginTick)
                .after(StatsSet::Expire)
                .after(StatsSet::Regenerate)
                .after(NavigationSet::TrackStatics)
                .after(CombatSet::Respawn)
                .before(OrdersSet::Orders)
                .before(AbilitiesSet::Toggles)
                .before(ActionsSet::HoldAtInputs),
            (slot_events, run_timers, unit_deaths, level_ups)
                .chain()
                .in_set(SimSet::Mode)
                .after(ProductionSet::Finish),
        ));
        registry.register_resource::<MatchEnd>();
        registry.register_resource::<ModeState>();
        registry.register_resource::<Choices>();
        registry.register_resource::<PlayerResources>();
        registry.register_resource::<Timers>();
        registry.register_resource::<UnansweredDeaths>();
        registry.register_resource::<UnansweredSlotEvents>();
    }

    /// The boundary the mode in `world` last asked for a save at, by `ctx.save()`, by the tick
    /// it comes before.
    pub fn save_asked(world: &World) -> Option<Tick> {
        world.get_resource::<SaveAsked>().map(|asked| asked.at())
    }

    /// The team of player `slot` in the match in `world`; `None` before the mode installs, or for
    /// a slot the session does not have.
    pub fn team_of(world: &World, slot: PlayerSlot) -> Option<Team> {
        ModeBook::of(world.get_non_send::<Ctx>()?)?.teams.of(slot)
    }

    /// Applies `effect`, which a call of `book`'s script queued in tick `now`.
    fn apply_effect(world: &mut World, book: &ModeBook, now: Tick, effect: ModeEffect) {
        match effect {
            ModeEffect::Timer {
                name,
                ticks,
                repeat,
                data,
            } => world
                .resource_mut::<Timers>()
                .set(now, name, ticks, repeat, data),
            ModeEffect::End(result) => {
                let tick = world.resource::<SimTick>().start();
                world.insert_resource(MatchEnd::new(tick, result));
            }
            ModeEffect::SpawnUnit { at, owner } => {
                book.spawn_owned(world, at, owner);
            }
            ModeEffect::SpawnGroup {
                team,
                path,
                from,
                units,
            } => book.spawn_group(world, team, path, from, &units),
            ModeEffect::Grant {
                unit,
                kind,
                rank,
                abilities,
            } => ModeBook::grant(world, unit, kind, rank, &abilities),
            ModeEffect::Respawn { unit, ticks } => {
                let entity = world.resource::<EntityIndex>().get(unit);
                let entity = entity.expect("a dead unit that stays is in the world");
                let at = now.after(ticks);
                world.entity_mut(entity).insert(Respawn { at });
            }
            ModeEffect::Learn { unit, slot } => {
                let entity = world.resource::<EntityIndex>().get(unit);
                let entity = entity.expect("a unit the view read is in the world");
                let slots = world.get_mut::<ActionSlots>(entity);
                slots.expect("a unit with ability slots").learn(slot);
            }
            ModeEffect::SetRelation { a, b, attitude } => {
                world
                    .resource_mut::<Relations>()
                    .set_attitude(a, b, attitude);
            }
            // A call's `now` is the end of its tick.
            ModeEffect::Save => world.insert_resource(SaveAsked::new(now)),
        }
    }

    /// Starts the match, before its first tick: the map's placed units spawn, on their paths and
    /// walking them as they name, then `on_match_start` runs. A timer it sets counts from the
    /// start. An error when the call fails: a match its mode cannot start would run without its
    /// rules.
    pub fn start(world: &mut World) -> Result<(), CallError> {
        let ctx = world.non_send::<Ctx>().clone();
        let book = ModeBook::of_match(&ctx);
        for placed in &book.placed {
            let at = SpawnAt {
                id: world.resource_mut::<IdAllocator>().allocate(),
                unit_type: placed.unit_type,
                team: placed.team,
                pos: placed.pos,
                angle: placed.angle,
            };
            match placed.path {
                None => book.spawn(world, at, ()),
                Some(PlacedPath { path, from: None }) => book.spawn(world, at, OnPath::new(path)),
                Some(PlacedPath {
                    path,
                    from: Some(from),
                }) => {
                    let walker = (OnPath::new(path), PathWalker::start(from, at.id));
                    book.spawn(world, at, walker)
                }
            };
        }
        if !book.schema.hooks.contains(Hook::OnMatchStart) {
            return Ok(());
        }
        let now = world.resource::<SimTick>().start();
        Calls::batch(world, &ctx, now, |call| {
            call.run(Pool::Mode, Hook::OnMatchStart, (call.ctx.clone(),))
        })
        .map_err(CallError::from_script)
    }
}

/// Runs `on_mode_input` for each mode input of the tick, in input order, from its player's pool. An
/// input whose name the mode does not declare, or whose value is not of its type, is ignored.
fn mode_inputs(
    world: &mut World,
    mut bodies: Local<'_, Vec<u8>>,
    mut inputs: Local<'_, Vec<Input>>,
) {
    let ctx = world.non_send::<Ctx>().clone();
    if !ModeBook::of_match(&ctx)
        .schema
        .hooks
        .contains(Hook::OnModeInput)
    {
        return;
    }
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
    let bodies = &*bodies;
    Calls::batch(world, &ctx, now, |call| {
        for input in &*inputs {
            let Some(decoded) = ModeInput::decode(&bodies[input.body.clone()], |name| {
                call.book().schema.input_type_named(name)
            }) else {
                continue;
            };
            let value = match decoded.value {
                InputValue::String(text) => Dynamic::from(ImmutableString::from(text)),
                InputValue::StringList(texts) => Dynamic::from_array(
                    texts
                        .into_iter()
                        .map(|text| Dynamic::from(ImmutableString::from(text)))
                        .collect(),
                ),
            };
            let name = ImmutableString::from(decoded.name);
            let args = (call.ctx.clone(), INT::from(input.slot.get()), name, value);
            // An input is answered once: one whose call finds the pool spent fails.
            let pool = Pool::Player(input.slot);
            if let ControlFlow::Break(error) = call.answer(None, pool, Hook::OnModeInput, args) {
                call.batch.record(None, Hook::OnModeInput, error);
            }
        }
    });
}

/// One mode input of the tick: its player's slot, and its body in the scratch buffer.
#[derive(Debug)]
struct Input {
    slot: PlayerSlot,
    body: Range<usize>,
}

/// Runs `on_player_join` for each player who took a slot in this tick, and `on_player_leave` for
/// each who left one, after those that wait from earlier ticks, in the order the log took them,
/// from the mode pool. One whose call finds the pool spent waits, with those after it, for a
/// later tick.
fn slot_events(world: &mut World) {
    // A write marks the state changed, and taking the resource out to scope it is one, so a tick
    // with no event to answer leaves it as it is.
    let events = world.resource::<TickInputs>().slot_events().is_empty();
    if events && world.resource::<UnansweredSlotEvents>().0.is_empty() {
        return;
    }
    world.resource_scope(|world, mut unanswered: Mut<'_, UnansweredSlotEvents>| {
        let events = world.resource::<TickInputs>().slot_events();
        unanswered.0.extend(events.iter().copied());
        let ctx = world.non_send::<Ctx>().clone();
        let hooks = ModeBook::of_match(&ctx).schema.hooks;
        let now = world.resource::<SimTick>().end();
        let answered = Calls::batch(world, &ctx, now, |call| {
            let mut answered = 0;
            for event in unanswered.0.iter() {
                let hook = match event.kind {
                    SlotEventKind::Joined => Hook::OnPlayerJoin,
                    SlotEventKind::Left => Hook::OnPlayerLeave,
                };
                if hooks.contains(hook) {
                    let args = (call.ctx.clone(), INT::from(event.slot.get()));
                    if call.answer(None, Pool::Mode, hook, args).is_break() {
                        break;
                    }
                }
                answered += 1;
            }
            answered
        });
        unanswered.0.answered(answered);
    });
}

/// Runs `on_timer` for each timer due in this tick's Mode stage, earliest first, from the mode
/// pool. A timer whose call finds the pool spent stays due, and fires in a later tick.
fn run_timers(world: &mut World) {
    let now = world.resource::<SimTick>().end();
    if world.resource::<Timers>().due(now).is_none() {
        return;
    }
    let ctx = world.non_send::<Ctx>().clone();
    Calls::batch(world, &ctx, now, |call| {
        let hooked = call.book().schema.hooks.contains(Hook::OnTimer);
        while let Some(timer) = call.batch.world().resource::<Timers>().due(now) {
            let name = ImmutableString::from(timer.name.as_str());
            if hooked {
                let data = timer
                    .data
                    .as_ref()
                    .map_or(Dynamic::UNIT, |data| data.to_dynamic(call.ctx.view()));
                let args = (call.ctx.clone(), name, data);
                if call
                    .answer(None, Pool::Mode, Hook::OnTimer, args)
                    .is_break()
                {
                    break;
                }
            }
            call.batch.world().resource_mut::<Timers>().fire();
        }
    });
}

/// Runs `on_unit_died` for each death of the tick, after those that wait from earlier ticks, in
/// the order they happened, from the mode pool: with the unit, its killer or `()`, and its
/// assisters; a killer gone by then is `()`, and an assister gone by then is left out. A death
/// whose call finds the pool spent waits, with those after it, for a later tick, and its unit
/// stays, dead, until then.
fn unit_deaths(world: &mut World, mut units: Local<'_, Vec<Option<Entity>>>) {
    let ctx = world.non_send::<Ctx>().clone();
    let hooks = ModeBook::of_match(&ctx).schema.hooks;
    if !hooks.contains(Hook::OnUnitDied) {
        return;
    }
    // A write marks the state changed, and taking the resource out to scope it is one, so a tick
    // with no death to answer leaves it as it is.
    if world.resource::<Deaths>().is_empty() && world.resource::<UnansweredDeaths>().is_empty() {
        return;
    }
    let now = world.resource::<SimTick>().end();
    world.resource_scope(|world, mut unanswered: Mut<'_, UnansweredDeaths>| {
        unanswered.extend(world.resource::<Deaths>().iter());
        let answered = Calls::batch(world, &ctx, now, |call| {
            let view = call.ctx.view().clone();
            let mut answered = 0;
            for death in unanswered.iter() {
                if let Some(unit) = view.unit(death.unit) {
                    let killer = death.killer.and_then(|id| view.unit(id));
                    let killer = killer.map_or(Dynamic::UNIT, Dynamic::from);
                    let assisters = death.assisters.iter().filter_map(|&id| view.unit(id));
                    let assisters: Array = assisters.map(Dynamic::from).collect();
                    let args = (call.ctx.clone(), Dynamic::from(unit), killer, assisters);
                    let unit = Some(death.unit);
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
        units.extend(unanswered.iter().map(|death| index.get(death.unit)));
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
        unanswered.answered(answered);
    });
}

/// Runs `on_level_up` for each level a unit reached, in the order reached, from the mode pool:
/// with the unit, the track's name and the level. A level that a call reaches joins the end, so a
/// chain of level-ups ends within the tick, as levels are finite. A level-up whose call finds the
/// pool spent waits, with those after it, for a later tick; one whose unit is gone by then runs
/// no call.
fn level_ups(world: &mut World, mut due: Local<'_, PendingCalls<LevelUp>>) {
    if !world.contains_resource::<LevelUps>() {
        return;
    }
    let ctx = world.non_send::<Ctx>().clone();
    let hooks = ModeBook::of_match(&ctx).schema.hooks;
    // A write marks the state changed, so a tick with no level-up writes nothing.
    if world.resource::<LevelUps>().0.is_empty() {
        return;
    }
    if !hooks.contains(Hook::OnLevelUp) {
        world.resource_mut::<LevelUps>().0.clear();
        return;
    }
    let now = world.resource::<SimTick>().end();
    loop {
        if world.resource::<LevelUps>().0.is_empty() {
            return;
        }
        due.clear();
        mem::swap(&mut *due, &mut world.resource_mut::<LevelUps>().0);
        let answered = Calls::batch(world, &ctx, now, |call| {
            let view = call.ctx.view().clone();
            let mut answered = 0;
            for &LevelUp { unit, track, level } in due.iter() {
                if let Some(handle) = view.unit(unit) {
                    let level = INT::from(level.get());
                    let args = (call.ctx.clone(), handle, view.track_name(track), level);
                    if call
                        .answer(Some(unit), Pool::Mode, Hook::OnLevelUp, args)
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

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position, StableId};

    use crate::mode::mode_book::ModeBook;
    use crate::scripts::ctx::Ctx;
    use crate::units::spawner::{SpawnAt, Spawner};
    use crate::units::team::Team;

    /// Spawns a unit of the mode's type `name` on `team` at `pos`, with the parts its type's
    /// kit gives, as the mode's own spawns do.
    pub fn spawn_typed(world: &mut World, name: &str, team: Team, pos: Position) -> StableId {
        let ctx = world.non_send::<Ctx>().clone();
        let book = ModeBook::of_match(&ctx);
        let unit_type = ctx
            .view()
            .unit_type_named(name)
            .filter(|&unit_type| book.kit(unit_type).is_some())
            .expect("a type of the mode, with a kit");
        let id = world.resource_mut::<IdAllocator>().allocate();
        let spawner = world.non_send::<Spawner>().clone();
        spawner.spawn(
            world,
            SpawnAt {
                id,
                unit_type,
                team,
                pos,
                angle: Num::ZERO,
            },
            None,
        );
        id
    }
}

#[cfg(test)]
mod tests;
