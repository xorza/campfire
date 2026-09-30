//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

use std::ops::Range;
use std::rc::Rc;

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::Local;
use bevy_ecs::world::{Mut, World};
use campfire_math::PlayerSlot;
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_script::{ScriptError, ScriptHost};
use campfire_sim::{
    Command, Position, SimSet, SimTick, StateRegistry, TickInputs, TickRate, Ticks,
};

use crate::combat::CombatSet;
use crate::combat::assist_window::AssistWindow;
use crate::combat::deaths::Deaths;
use crate::mode::calls::Calls;
use crate::mode::error::ModeError;
use crate::mode::map_data::{GroundPoint, MapData};
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_ctx::ModeCtx;
use crate::mode::mode_input::{InputValue, ModeInput};
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_state::ModeState;
use crate::mode::picks::{Pick, Picks};
use crate::mode::player_resources::PlayerResources;
use crate::mode::team_manifest::TeamManifest;
use crate::mode::timers::Timers;
use crate::navigation::lanes::Lanes;
use crate::navigation::on_lane::OnLane;
use crate::orders::OrdersSet;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::units::UnitsSet;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::unit::Unit;
use crate::vision::Vision;

pub(crate) mod calls;
pub(crate) mod error;
pub(crate) mod hero_index;
pub(crate) mod map_data;
pub(crate) mod mode_book;
pub(crate) mod mode_ctx;
pub(crate) mod mode_data;
pub(crate) mod mode_input;
pub(crate) mod mode_schema;
pub(crate) mod mode_setup;
pub(crate) mod mode_state;
pub(crate) mod picks;
pub(crate) mod player_resources;
pub(crate) mod roster;
pub(crate) mod spell_index;
pub(crate) mod team_manifest;
pub(crate) mod timers;
pub(crate) mod unit_kit;

/// The mode of a match: the core's rules above the capabilities. Every match installs it after
/// its capabilities.
#[derive(Debug)]
pub struct Mode;

impl Mode {
    /// Adds the mode of `setup`, which passed `Mode::check` when its package loaded, to a match
    /// whose capabilities are installed and whose unit types, abilities and AI are loaded: in
    /// Inputs, the players' mode inputs run `on_mode_input`; in Mode, due timers run `on_timer`,
    /// then the tick's deaths run `on_unit_died`. The map's lanes and grid become the match's,
    /// and the mode's `assist_window_ms` combat's.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        setup: ModeSetup<'_>,
    ) -> Result<(), ModeError> {
        let view = world.non_send::<View>().clone();
        let rate = *world.resource::<TickRate>();
        let lanes = Mode::lanes(setup.map);
        let grid = setup.map.grid;
        // A window past what ticks can count covers the whole match.
        let assist_window = setup
            .data
            .assist_window_ms
            .map(|ms| rate.ticks(ms).unwrap_or(Ticks::new(u64::MAX)));
        let book = {
            let mut host = world.non_send_mut::<ScriptHost>();
            ModeCtx::register(host.engine_mut());
            ModeBook::new(setup, rate, &host, &view, &lanes)?
        };
        if let Some(grid) = grid {
            Vision::load_grid(world, grid, book.teams.count());
        }
        if let Some(window) = assist_window {
            world.insert_resource(AssistWindow(window));
        }
        view.set_names(Rc::clone(&book.teams), lanes.shared_names());
        world.insert_resource(lanes);
        world.insert_resource(ModeState(book.schema.state_initial.clone()));
        let players = book.teams.players() as usize;
        world.insert_resource(Picks(vec![Pick::default(); players]));
        world.insert_resource(PlayerResources::default());
        world.insert_resource(Timers::default());
        world.insert_non_send(ModeCtx::new(view, book));
        schedule.add_systems((
            mode_inputs
                .in_set(SimSet::Inputs)
                .after(UnitsSet::BeginTick)
                .after(CombatSet::Respawn)
                .before(OrdersSet::Orders),
            (run_timers, unit_deaths).chain().in_set(SimSet::Mode),
        ));
        registry.register_resource::<ModeState>();
        registry.register_resource::<Picks>();
        registry.register_resource::<PlayerResources>();
        registry.register_resource::<Timers>();
        Ok(())
    }

    /// The lanes of `map`, which passed the check.
    fn lanes(map: &MapData) -> Lanes {
        let paths: Vec<(&str, Vec<Position>)> = map
            .lanes
            .iter()
            .map(|lane| {
                let points = lane.points.iter();
                let points = points.map(|point| point.position().expect("the check passed"));
                (lane.name.as_str(), points.collect())
            })
            .collect();
        Lanes::new(
            paths
                .iter()
                .map(|(name, points)| (*name, points.as_slice())),
        )
    }

    /// The team of player `slot` in the match in `world`; `None` before the mode installs, or for
    /// a slot the session does not have.
    pub fn team_of(world: &World, slot: PlayerSlot) -> Option<Team> {
        world.get_non_send::<ModeCtx>()?.book().teams.of(slot)
    }

    /// Checks what the mode names against what it has: its playing teams, of which none is
    /// named `neutral` and no two share a name, fewer than `Team::LIMIT` with the neutral one; and
    /// its map, whose lanes each have a waypoint and a name of their own, whose every
    /// playing team has a hero spawn, and whose structures and neutral spawns name unit types
    /// `unit_type` knows, and teams and lanes the mode has. Every point is within the world's
    /// bound.
    pub fn check(
        teams: &[TeamManifest],
        map: &MapData,
        unit_type: impl Fn(&str) -> bool,
    ) -> Result<(), ModeError> {
        for (at, team) in teams.iter().enumerate() {
            if team.name == Teams::NEUTRAL {
                return Err(ModeError::NeutralTeam);
            }
            if teams[..at].iter().any(|other| other.name == team.name) {
                return Err(ModeError::RepeatedName(team.name.clone()));
            }
        }
        if teams.len() >= Team::LIMIT {
            return Err(ModeError::TooManyTeams);
        }
        let in_bounds = |point: &GroundPoint| point.position().ok_or(ModeError::OutOfBounds);
        for (at, lane) in map.lanes.iter().enumerate() {
            if map.lanes[..at].iter().any(|other| other.name == lane.name) {
                return Err(ModeError::RepeatedName(lane.name.clone()));
            }
            if lane.points.is_empty() {
                return Err(ModeError::EmptyLane(lane.name.clone()));
            }
            for point in &lane.points {
                in_bounds(point)?;
            }
        }
        for team in teams {
            let spawn = map.spawns.get(&team.name);
            in_bounds(spawn.ok_or_else(|| ModeError::NoSpawn(team.name.clone()))?)?;
        }
        let team_known =
            |name: &str| name == Teams::NEUTRAL || teams.iter().any(|team| team.name == name);
        for structure in &map.structures {
            if !unit_type(&structure.unit_type) {
                return Err(ModeError::UnknownUnitType(structure.unit_type.clone()));
            }
            if !team_known(&structure.team) {
                return Err(ModeError::UnknownTeam(structure.team.clone()));
            }
            if let Some(lane) = &structure.lane
                && !map.lanes.iter().any(|known| known.name == *lane)
            {
                return Err(ModeError::UnknownLane(lane.clone()));
            }
            in_bounds(&structure.pos)?;
        }
        for spawn in &map.neutral_spawns {
            if !unit_type(&spawn.unit_type) {
                return Err(ModeError::UnknownUnitType(spawn.unit_type.clone()));
            }
            in_bounds(&spawn.pos)?;
        }
        Ok(())
    }

    /// Starts the match, before its first tick: the map's structures spawn, then
    /// `on_match_start` runs. A timer it sets counts from the start. An error when the call
    /// fails: a match its mode cannot start would run without its rules.
    pub fn start(world: &mut World) -> Result<(), CallError> {
        let ctx = world.non_send::<ModeCtx>().clone();
        for structure in &ctx.book().structures {
            let book = ctx.book();
            let (unit_type, team, pos) = (structure.unit_type, structure.team, structure.pos);
            let entity = book.spawn(world, unit_type, team, pos, ());
            if let Some(lane) = structure.lane {
                world.entity_mut(entity).insert(OnLane::new(lane));
            }
        }
        if !ctx.book().schema.hooks.contains(Hook::OnMatchStart) {
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
    let ctx = world.non_send::<ModeCtx>().clone();
    if !ctx.book().schema.hooks.contains(Hook::OnModeInput) {
        return;
    }
    bodies.clear();
    inputs.clear();
    for input in world.resource::<TickInputs>().iter() {
        for body in Command::bodies(input.payload, ModeInput::CAPABILITY) {
            let start = bodies.len();
            bodies.extend_from_slice(body);
            inputs.push(Input {
                slot: input.slot,
                body: start..bodies.len(),
            });
        }
    }
    if inputs.is_empty() {
        return;
    }
    let now = world.resource::<SimTick>().start();
    let bodies = &*bodies;
    Calls::batch(world, &ctx, now, |call| {
        for input in &*inputs {
            let Some(decoded) = ModeInput::decode(&bodies[input.body.clone()], |name| {
                call.ctx.book().schema.input_type(name)
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
            if let Err(error) = call.run(Pool::Player(input.slot), Hook::OnModeInput, args) {
                let error = CallError::from_script(error);
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

/// Runs `on_timer` for each timer due in this tick's Mode stage, earliest first, from the mode
/// pool. A timer whose call finds the pool spent stays due, and fires in a later tick.
fn run_timers(world: &mut World) {
    let now = world.resource::<SimTick>().end();
    if world.resource::<Timers>().due(now).is_none() {
        return;
    }
    let ctx = world.non_send::<ModeCtx>().clone();
    Calls::batch(world, &ctx, now, |call| {
        while let Some(timer) = call.batch.world().resource::<Timers>().due(now) {
            let name = ImmutableString::from(timer.name.as_str());
            let data = timer
                .data
                .as_ref()
                .map_or(Dynamic::UNIT, |data| data.to_dynamic(call.ctx.view()));
            if call.ctx.book().schema.hooks.contains(Hook::OnTimer) {
                let args = (call.ctx.clone(), name, data);
                match call.run(Pool::Mode, Hook::OnTimer, args) {
                    Ok(()) => {}
                    Err(ScriptError::TickBudget) => break,
                    Err(error) => {
                        let error = CallError::from_script(error);
                        call.batch.record(None, Hook::OnTimer, error);
                    }
                }
            }
            call.batch.world().resource_mut::<Timers>().fire();
        }
    });
}

/// Runs `on_unit_died` for each death of the tick, in the order they happened, from the mode
/// pool: with the unit, its killer or `()`, and its assisters.
fn unit_deaths(world: &mut World) {
    let ctx = world.non_send::<ModeCtx>().clone();
    if !ctx.book().schema.hooks.contains(Hook::OnUnitDied) || world.resource::<Deaths>().is_empty()
    {
        return;
    }
    let now = world.resource::<SimTick>().end();
    world.resource_scope(|world, deaths: Mut<'_, Deaths>| {
        Calls::batch(world, &ctx, now, |call| {
            let handle = |id| Dynamic::from(Unit::new(id, call.ctx.view().clone()));
            for death in deaths.iter() {
                let killer = death.killer.map_or(Dynamic::UNIT, handle);
                let assisters: Array = death.assisters.iter().map(|&id| handle(id)).collect();
                let args = (call.ctx.clone(), handle(death.unit), killer, assisters);
                if let Err(error) = call.run(Pool::Mode, Hook::OnUnitDied, args) {
                    let error = CallError::from_script(error);
                    call.batch.record(Some(death.unit), Hook::OnUnitDied, error);
                }
            }
        });
    });
}

#[cfg(test)]
mod tests;
