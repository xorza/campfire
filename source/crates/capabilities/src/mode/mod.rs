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
    Command, EntityIndex, Position, SimSet, SimTick, StateRegistry, Tick, TickInputs, TickRate,
    Ticks,
};

use crate::abilities::ability_slots::AbilitySlots;
use crate::combat::CombatSet;
use crate::combat::assist_window::AssistWindow;
use crate::combat::attack_kind::AttackKind;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::damage_weigher::DamageWeigher;
use crate::combat::deaths::Deaths;
use crate::combat::respawn::Respawn;
use crate::mode::calls::Calls;
use crate::mode::error::ModeError;
use crate::mode::map_data::{GroundPoint, MapData};
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_effect::ModeEffect;
use crate::mode::mode_input::{InputValue, ModeInput};
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_state::ModeState;
use crate::mode::picks::{Pick, Picks};
use crate::mode::player_resources::PlayerResources;
use crate::mode::relation_data::RelationData;
use crate::mode::team_manifest::TeamManifest;
use crate::mode::timers::Timers;
use crate::navigation::Navigation;
use crate::navigation::on_path::OnPath;
use crate::navigation::paths::Paths;
use crate::orders::OrdersSet;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::stats::Stats;
use crate::stats::pool_book::PoolBook;
use crate::stats::stat_book::StatBook;
use crate::units::relations::Relations;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::{Units, UnitsSet};
use crate::vision::Vision;

pub(crate) mod avatar_index;
pub(crate) mod calls;
pub(crate) mod error;
pub(crate) mod game_map;
pub(crate) mod loadout_index;
pub(crate) mod map_data;
pub(crate) mod match_end;
pub(crate) mod mode_api;
pub(crate) mod mode_book;
pub(crate) mod mode_data;
pub(crate) mod mode_effect;
pub(crate) mod mode_input;
pub(crate) mod mode_schema;
pub(crate) mod mode_setup;
pub(crate) mod mode_state;
pub(crate) mod picks;
pub(crate) mod player_resources;
pub(crate) mod relation_data;
pub(crate) mod roster;
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
    /// then the tick's deaths run `on_unit_died`. The map's bounds, paths and grid become the match's,
    /// and the mode's `[combat]`, `attack_kind` and `calc_damage` combat's.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        setup: ModeSetup<'_>,
    ) -> Result<(), ModeError> {
        let view = world.non_send::<View>().clone();
        let rate = *world.resource::<TickRate>();
        let paths = Mode::paths(setup.map);
        let bounds = setup.map.bounds;
        let grid = setup.map.grid().expect("the check passed");
        let pathing = setup.map.pathing().expect("the check passed");
        let walkers = setup.walkers.clone();
        // A window past what ticks can count covers the whole match.
        let assist_window = setup
            .data
            .combat
            .assist_window_ms
            .map(|ms| rate.ticks(ms).unwrap_or(Ticks::new(u64::MAX)));
        let attack_kind = setup.data.attack_kind.as_ref().map(|name| {
            view.damage_kind(name.as_str())
                .expect("the load checked the attack kind")
        });
        let types = setup
            .unit_types
            .iter()
            .map(|setup| (setup.unit_type, &setup.stats));
        let stats = StatBook::new(&setup.data.stats, types, rate, setup.max_move_speed)
            .ok_or(ModeError::StatValue)?
            .with_order(setup.stat_order.clone());
        let pools = &setup.data.pools;
        let bindings = CombatBindings::new(&setup.data.combat, pools, &stats);
        let pool_book = PoolBook::new(pools, &stats);
        let tags = view.types_mut().tag_book(&setup.data.tags);
        let data = setup.data;
        let book = ModeBook::new(setup, world.non_send::<ScriptHost>(), &view, &paths)?;
        let mut relations = Relations::default();
        for relation in &data.relations {
            let [a, b] = relation
                .teams
                .each_ref()
                .map(|name| book.teams.named(name).expect("the check passed"));
            relations.set(a, b, relation.relation, relation.vision);
        }
        world.insert_resource(relations);
        world.insert_resource(bindings);
        Stats::load(world, stats, pool_book);
        Units::load_tags(world, tags);
        if let Some(grid) = grid {
            Vision::load_grid(world, grid, book.teams.count());
        }
        if let Some(pathing) = pathing {
            Navigation::load_pathing(world, pathing, walkers);
        }
        if let Some(window) = assist_window {
            world.insert_resource(AssistWindow(window));
        }
        if let Some(kind) = attack_kind {
            world.insert_resource(AttackKind(kind));
        }
        view.set_names(Rc::clone(&book.teams), paths.shared_names());
        world.insert_resource(paths);
        world.insert_resource(bounds);
        world.insert_resource(ModeState(book.schema.state_initial.clone()));
        let players = book.teams.players() as usize;
        world.insert_resource(Picks(vec![Pick::default(); players]));
        world.insert_resource(PlayerResources::default());
        world.insert_resource(Timers::default());
        let weighs = book.schema.hooks.contains(Hook::CalcDamage);
        let ctx = world.non_send::<Ctx>().clone();
        ctx.set_mode(book);
        if weighs {
            world.insert_non_send(DamageWeigher::new(move |batch, damage| {
                Calls::weigh(batch, &ctx, damage)
            }));
        }
        schedule.add_systems((
            mode_inputs
                .in_set(SimSet::Inputs)
                .after(UnitsSet::BeginTick)
                .after(CombatSet::Respawn)
                .before(OrdersSet::Orders),
            (run_timers, unit_deaths).chain().in_set(SimSet::Mode),
        ));
        registry.register_resource::<MatchEnd>();
        registry.register_resource::<ModeState>();
        registry.register_resource::<Picks>();
        registry.register_resource::<PlayerResources>();
        registry.register_resource::<Timers>();
        Ok(())
    }

    /// The paths of `map`, which passed the check.
    fn paths(map: &MapData) -> Paths {
        let paths: Vec<(&str, Vec<Position>)> = map
            .paths
            .iter()
            .map(|path| {
                let points = path.points.iter();
                let points = points.map(|point| point.position().expect("the check passed"));
                (path.name.as_str(), points.collect())
            })
            .collect();
        Paths::new(
            paths
                .iter()
                .map(|(name, points)| (*name, points.as_slice())),
        )
    }

    /// The team of player `slot` in the match in `world`; `None` before the mode installs, or for
    /// a slot the session does not have.
    pub fn team_of(world: &World, slot: PlayerSlot) -> Option<Team> {
        world.get_non_send::<Ctx>()?.mode()?.teams.of(slot)
    }

    /// Applies `effect`, which a call of `book`'s script queued in tick `now`; a passive it gives
    /// reads its params through `frame`, the call's.
    pub(crate) fn apply_effect(
        world: &mut World,
        book: &ModeBook,
        now: Tick,
        effect: ModeEffect,
        frame: &Frame,
    ) {
        match effect {
            ModeEffect::Timer {
                name,
                ticks,
                repeat,
                data,
            } => world
                .resource_mut::<Timers>()
                .set(now, name, ticks, repeat, data),
            ModeEffect::SpawnAvatars => book.spawn_avatars(world, frame),
            ModeEffect::End(result) => {
                let tick = world.resource::<SimTick>().start();
                world.insert_resource(MatchEnd::new(tick, result));
            }
            ModeEffect::SpawnUnit {
                unit_type,
                team,
                pos,
            } => drop(book.spawn(world, unit_type, team, pos, ())),
            ModeEffect::SpawnGroup { team, path, types } => {
                book.spawn_group(world, team, path, &types);
            }
            ModeEffect::Respawn { unit, ticks } => {
                let entity = world.resource::<EntityIndex>().get(unit);
                let entity = entity.expect("a dead unit that stays is in the world");
                let at = now.after(ticks);
                world.entity_mut(entity).insert(Respawn { at });
            }
            ModeEffect::Learn { unit, slot } => {
                let entity = world.resource::<EntityIndex>().get(unit);
                let entity = entity.expect("a unit the view read is in the world");
                let slots = world.get_mut::<AbilitySlots>(entity);
                slots.expect("a unit with ability slots").learn(slot);
            }
            ModeEffect::SetRelation { a, b, attitude } => {
                world
                    .resource_mut::<Relations>()
                    .set_attitude(a, b, attitude);
            }
        }
    }

    /// Checks what the mode names against what it has: its teams, no two of which share a name,
    /// at most `Team::LIMIT`; its relations, each of two teams it has, and no pair twice; and its
    /// map, whose paths each have a waypoint and a name of their own, whose every playing team
    /// has an avatar spawn, and whose structures and neutral spawns name unit types `unit_type`
    /// knows, and teams and paths the mode has. Every point is within the map's bounds, and its
    /// grid, if it has one, makes a grid of them.
    pub fn check(
        teams: &[TeamManifest],
        relations: &[RelationData],
        map: &MapData,
        unit_type: impl Fn(&str) -> bool,
    ) -> Result<(), ModeError> {
        for (at, team) in teams.iter().enumerate() {
            if teams[..at].iter().any(|other| other.name == team.name) {
                return Err(ModeError::RepeatedName(team.name.clone()));
            }
        }
        if teams.len() > Team::LIMIT {
            return Err(ModeError::TooManyTeams);
        }
        let team_known = |name: &str| teams.iter().any(|team| team.name == name);
        for (at, relation) in relations.iter().enumerate() {
            let [a, b] = &relation.teams;
            if let Some(unknown) = [a, b].into_iter().find(|name| !team_known(name)) {
                return Err(ModeError::UnknownTeam(unknown.clone()));
            }
            let pair = |data: &RelationData| {
                let [x, y] = &data.teams;
                (x == a && y == b) || (x == b && y == a)
            };
            if a == b || relations[..at].iter().any(pair) {
                return Err(ModeError::RepeatedRelation(a.clone(), b.clone()));
            }
        }
        map.grid()?;
        map.pathing()?;
        let in_bounds = |point: &GroundPoint| match point.position() {
            Some(pos) if map.bounds.contains(pos) => Ok(()),
            _ => Err(ModeError::OutOfBounds),
        };
        for (at, path) in map.paths.iter().enumerate() {
            if map.paths[..at].iter().any(|other| other.name == path.name) {
                return Err(ModeError::RepeatedName(path.name.clone()));
            }
            if path.points.is_empty() {
                return Err(ModeError::EmptyPath(path.name.clone()));
            }
            for point in &path.points {
                in_bounds(point)?;
            }
        }
        for team in teams.iter().filter(|team| team.slots > 0) {
            let spawn = map.spawns.get(&team.name);
            in_bounds(spawn.ok_or_else(|| ModeError::NoSpawn(team.name.clone()))?)?;
        }
        for structure in &map.structures {
            if !unit_type(&structure.unit_type) {
                return Err(ModeError::UnknownUnitType(structure.unit_type.clone()));
            }
            if !team_known(&structure.team) {
                return Err(ModeError::UnknownTeam(structure.team.clone()));
            }
            if let Some(path) = &structure.path
                && !map.paths.iter().any(|known| known.name == *path)
            {
                return Err(ModeError::UnknownPath(path.clone()));
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
        let ctx = world.non_send::<Ctx>().clone();
        let book = ctx.mode().expect("a match with a mode");
        for structure in &book.structures {
            let (unit_type, team, pos) = (structure.unit_type, structure.team, structure.pos);
            let entity = book.spawn(world, unit_type, team, pos, ());
            if let Some(path) = structure.path {
                world.entity_mut(entity).insert(OnPath::new(path));
            }
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
    if !ctx
        .mode()
        .expect("a match with a mode")
        .schema
        .hooks
        .contains(Hook::OnModeInput)
    {
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
                call.book().schema.input_type(name)
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
    let ctx = world.non_send::<Ctx>().clone();
    Calls::batch(world, &ctx, now, |call| {
        while let Some(timer) = call.batch.world().resource::<Timers>().due(now) {
            let name = ImmutableString::from(timer.name.as_str());
            let data = timer
                .data
                .as_ref()
                .map_or(Dynamic::UNIT, |data| data.to_dynamic(call.ctx.view()));
            if call.book().schema.hooks.contains(Hook::OnTimer) {
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
    let ctx = world.non_send::<Ctx>().clone();
    if !ctx
        .mode()
        .expect("a match with a mode")
        .schema
        .hooks
        .contains(Hook::OnUnitDied)
        || world.resource::<Deaths>().is_empty()
    {
        return;
    }
    let now = world.resource::<SimTick>().end();
    world.resource_scope(|world, deaths: Mut<'_, Deaths>| {
        Calls::batch(world, &ctx, now, |call| {
            let view = call.ctx.view().clone();
            let handle = |id| {
                let unit = view.unit(id);
                Dynamic::from(unit.expect("`Deaths` names only units that exist in its tick"))
            };
            for death in deaths.iter() {
                let killer = death.killer.map_or(Dynamic::UNIT, handle);
                let assisters: Array = death.assisters.iter().map(|&id| handle(id)).collect();
                let unit = death.fallen.unit;
                let args = (call.ctx.clone(), handle(unit), killer, assisters);
                if let Err(error) = call.run(Pool::Mode, Hook::OnUnitDied, args) {
                    let error = CallError::from_script(error);
                    call.batch.record(Some(unit), Hook::OnUnitDied, error);
                }
            }
        });
    });
}

#[cfg(test)]
mod tests;
