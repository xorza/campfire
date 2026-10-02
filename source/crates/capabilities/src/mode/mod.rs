//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

use std::mem;
use std::ops::Range;
use std::rc::Rc;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::Local;
use bevy_ecs::world::{Mut, World};
use campfire_math::PlayerSlot;
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_script::{ScriptError, ScriptHost};
use campfire_sim::{
    Command, EntityIndex, IdAllocator, Position, SimSet, SimTick, StateRegistry, Tick, TickInputs,
    TickRate, Ticks,
};

use crate::actions::action_slots::ActionSlots;
use crate::combat::CombatSet;
use crate::combat::assist_window::AssistWindow;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::damage_weigher::DamageWeigher;
use crate::combat::deaths::Deaths;
use crate::combat::kept::Kept;
use crate::combat::respawn::Respawn;
use crate::mode::calls::Calls;
use crate::mode::choices::Choices;
use crate::mode::error::ModeError;
use crate::mode::map_data::{MapData, MapPoint};
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_book::{ModeBook, SpawnAt};
use crate::mode::mode_effect::ModeEffect;
use crate::mode::mode_input::{InputValue, ModeInput};
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_state::ModeState;
use crate::mode::player_resources::PlayerResources;
use crate::mode::relation_data::RelationData;
use crate::mode::team_manifest::TeamManifest;
use crate::mode::timers::Timers;
use crate::mode::unanswered_deaths::UnansweredDeaths;
use crate::navigation::Navigation;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::orders::OrdersSet;
use crate::production::Production;
use crate::progression::level_ups::{LevelUp, LevelUps};
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::stats::Stats;
use crate::stats::pool_book::PoolBook;
use crate::stats::stat_book::StatBook;
use crate::units::body::Body;
use crate::units::relations::Relations;
use crate::units::script_view::View;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::{Units, UnitsSet};
use crate::vision::Vision;

pub(crate) mod calls;
pub(crate) mod choice_book;
pub(crate) mod choice_data;
pub(crate) mod choices;
pub(crate) mod error;
pub(crate) mod game_map;
pub(crate) mod map_data;
pub(crate) mod marker;
pub(crate) mod match_end;
pub(crate) mod mode_api;
pub(crate) mod mode_book;
pub(crate) mod mode_data;
pub(crate) mod mode_effect;
pub(crate) mod mode_input;
pub(crate) mod mode_schema;
pub(crate) mod mode_setup;
pub(crate) mod mode_state;
pub(crate) mod new_unit;
pub(crate) mod offer;
pub(crate) mod player_resources;
pub(crate) mod relation_data;
pub(crate) mod resource_id;
pub(crate) mod roster;
pub(crate) mod team_manifest;
pub(crate) mod timers;
pub(crate) mod unanswered_deaths;
pub(crate) mod unit_kit;

/// The mode of a match: the core's rules above the capabilities. Every match installs it after
/// its capabilities.
#[derive(Debug)]
pub struct Mode;

impl Mode {
    /// Adds the mode of `setup`, which passed `Mode::check` when its package loaded, to a match
    /// whose capabilities are installed and whose unit types, abilities and AI are loaded: in
    /// Inputs, the players' mode inputs run `on_mode_input`; in Mode, the trains whose time ended
    /// spawn, due timers run `on_timer`, the tick's deaths run `on_unit_died`, and the levels
    /// reached run `on_level_up`. The map's metric, bounds, paths and grid become
    /// the match's, and the mode's `[combat]` and `calc_damage` combat's.
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
        let metric = setup.map.metric;
        let grid = setup.map.grid().expect("the check passed");
        let pathing = setup.map.pathing().expect("the check passed");
        let walkers = setup.walkers.clone();
        // A window past what ticks can count covers the whole match.
        let assist_window = setup
            .data
            .combat
            .assist_window_ms
            .map(|ms| rate.ticks(ms).unwrap_or(Ticks::new(u64::MAX)));
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
        let tags = Mode::tag_book(&view, &setup);
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
        view.set_names(Rc::clone(&book.teams), paths.shared_names());
        world.insert_resource(paths);
        world.insert_resource(bounds);
        world.insert_resource(metric);
        world.insert_resource(ModeState(book.schema.state_initial.clone()));
        let players = book.teams.players() as usize;
        world.insert_resource(book.choices.empty(players));
        world.insert_resource(PlayerResources::new(players, view.resource_count()));
        world.insert_resource(Timers::default());
        world.insert_resource(UnansweredDeaths::default());
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
            (
                Production::finish_trains,
                run_timers,
                unit_deaths,
                level_ups,
            )
                .chain()
                .in_set(SimSet::Mode),
        ));
        registry.register_resource::<MatchEnd>();
        registry.register_resource::<ModeState>();
        registry.register_resource::<Choices>();
        registry.register_resource::<PlayerResources>();
        registry.register_resource::<Timers>();
        registry.register_resource::<UnansweredDeaths>();
        Ok(())
    }

    /// The book of the mode's tags: their effects, and each unit type's own tags, the name of
    /// the layer it moves on among them, when the mode names its layers.
    fn tag_book(view: &View, setup: &ModeSetup<'_>) -> TagBook {
        let mut types = view.types_mut();
        let layers = &setup.data.navigation.layers;
        for unit_type in &setup.unit_types {
            let layer = Body::layer_of(unit_type.kit.body.as_ref());
            if let Some(name) = layers.get(usize::from(layer.index())) {
                let tag = types
                    .declare(name.as_str())
                    .expect("the match declared every tag its packages name");
                types.give_tag(unit_type.unit_type, tag);
            }
        }
        types.tag_book(&setup.data.tags)
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
            ModeEffect::End(result) => {
                let tick = world.resource::<SimTick>().start();
                world.insert_resource(MatchEnd::new(tick, result));
            }
            ModeEffect::SpawnUnit { at, owner } => {
                book.spawn_owned(world, at, owner, Some(frame));
            }
            ModeEffect::SpawnGroup {
                team,
                path,
                from,
                units,
            } => book.spawn_group(world, team, path, from, &units, frame),
            ModeEffect::Grant {
                unit,
                kind,
                abilities,
            } => book.grant(world, unit, kind, &abilities),
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
        }
    }

    /// Checks what the mode names against what it has: its teams, no two of which share a name,
    /// at most `Team::LIMIT`; its relations, each of two teams it has, and no pair twice; and its
    /// map, as `check_map` does.
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
        Mode::check_map(map, team_known, unit_type)
    }

    /// Checks `map`: its grids make grids of its bounds; its paths each have a waypoint and a
    /// name of their own; its placed units are of unit types `unit_type` knows and teams
    /// `team_known` knows, on paths it has, and walk one only from an end of a path they name;
    /// its markers have names of their own, teams it knows, and a point or a region within its
    /// bounds, not both. Every point fits its metric and is within its bounds.
    fn check_map(
        map: &MapData,
        team_known: impl Fn(&str) -> bool,
        unit_type: impl Fn(&str) -> bool,
    ) -> Result<(), ModeError> {
        map.grid()?;
        map.pathing()?;
        let point = |point: &MapPoint| match point.position() {
            _ if !point.fits(map.metric) => Err(ModeError::PointShape),
            Some(pos) if map.bounds.contains(pos) => Ok(()),
            _ => Err(ModeError::OutOfBounds),
        };
        let path_known = |name: &String| map.paths.iter().any(|path| path.name == *name);
        for (at, path) in map.paths.iter().enumerate() {
            if map.paths[..at].iter().any(|other| other.name == path.name) {
                return Err(ModeError::RepeatedName(path.name.clone()));
            }
            if path.points.is_empty() {
                return Err(ModeError::EmptyPath(path.name.clone()));
            }
            path.points.iter().try_for_each(point)?;
        }
        for unit in &map.units {
            if !unit_type(&unit.unit_type) {
                return Err(ModeError::UnknownUnitType(unit.unit_type.clone()));
            }
            if !team_known(&unit.team) {
                return Err(ModeError::UnknownTeam(unit.team.clone()));
            }
            match (&unit.path, unit.from) {
                (Some(path), _) if !path_known(path) => {
                    return Err(ModeError::UnknownPath(path.clone()));
                }
                (None, Some(_)) => return Err(ModeError::NoPathToWalk(unit.unit_type.clone())),
                _ => {}
            }
            point(&unit.pos)?;
        }
        for (at, marker) in map.markers.iter().enumerate() {
            if map.markers[..at]
                .iter()
                .any(|other| other.name == marker.name)
            {
                return Err(ModeError::RepeatedName(marker.name.clone()));
            }
            if let Some(team) = marker.team.as_ref().filter(|team| !team_known(team)) {
                return Err(ModeError::UnknownTeam(team.clone()));
            }
            if let Some(region) = marker.region
                && (marker.pos.is_some() || !region.holds(map.metric, map.bounds))
            {
                return Err(ModeError::Region(marker.name.clone()));
            }
            marker.pos.iter().try_for_each(point)?;
        }
        Ok(())
    }

    /// Starts the match, before its first tick: the map's placed units spawn, on their paths and
    /// walking them as they name, then `on_match_start` runs. A timer it sets counts from the
    /// start. An error when the call fails: a match its mode cannot start would run without its
    /// rules.
    pub fn start(world: &mut World) -> Result<(), CallError> {
        let ctx = world.non_send::<Ctx>().clone();
        let book = ctx.mode().expect("a match with a mode");
        for placed in &book.placed {
            let at = SpawnAt {
                id: world.resource_mut::<IdAllocator>().allocate(),
                unit_type: placed.unit_type,
                team: placed.team,
                pos: placed.pos,
            };
            let entity = book.spawn(world, at, (), None);
            let mut entity = world.entity_mut(entity);
            if let Some(path) = placed.path {
                entity.insert(OnPath::new(path));
            }
            if let Some(from) = placed.from {
                entity.insert(PathWalker::start(from));
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

/// Runs `on_unit_died` for each death of the tick, after those that wait from earlier ticks, in
/// the order they happened, from the mode pool: with the unit, its killer or `()`, and its
/// assisters; a killer gone by then is `()`, and an assister gone by then is left out. A death
/// whose call finds the pool spent waits, with those after it, for a later tick, and its unit
/// stays, dead, until then.
fn unit_deaths(world: &mut World, mut units: Local<'_, Vec<Option<Entity>>>) {
    let ctx = world.non_send::<Ctx>().clone();
    let hooks = ctx.mode().expect("a match with a mode").schema.hooks;
    if !hooks.contains(Hook::OnUnitDied) {
        return;
    }
    let now = world.resource::<SimTick>().end();
    world.resource_scope(|world, mut unanswered: Mut<'_, UnansweredDeaths>| {
        unanswered.extend(world.resource::<Deaths>().iter());
        if unanswered.is_empty() {
            return;
        }
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
                    match call.run(Pool::Mode, Hook::OnUnitDied, args) {
                        Ok(()) => {}
                        Err(ScriptError::TickBudget) => break,
                        Err(error) => {
                            let error = CallError::from_script(error);
                            call.batch.record(Some(death.unit), Hook::OnUnitDied, error);
                        }
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
fn level_ups(world: &mut World, mut due: Local<'_, Vec<LevelUp>>) {
    if !world.contains_resource::<LevelUps>() {
        return;
    }
    let ctx = world.non_send::<Ctx>().clone();
    let hooks = ctx.mode().expect("a match with a mode").schema.hooks;
    if !hooks.contains(Hook::OnLevelUp) {
        world.resource_mut::<LevelUps>().0.clear();
        return;
    }
    let now = world.resource::<SimTick>().end();
    loop {
        due.clear();
        mem::swap(&mut *due, &mut world.resource_mut::<LevelUps>().0);
        if due.is_empty() {
            return;
        }
        let answered = Calls::batch(world, &ctx, now, |call| {
            let view = call.ctx.view().clone();
            let mut answered = 0;
            for &LevelUp { unit, track, level } in &*due {
                if let Some(handle) = view.unit(unit) {
                    let level = INT::from(level.get());
                    let args = (call.ctx.clone(), handle, view.track_name(track), level);
                    match call.run(Pool::Mode, Hook::OnLevelUp, args) {
                        Ok(()) => {}
                        Err(ScriptError::TickBudget) => break,
                        Err(error) => {
                            let error = CallError::from_script(error);
                            call.batch.record(Some(unit), Hook::OnLevelUp, error);
                        }
                    }
                }
                answered += 1;
            }
            answered
        });
        if answered < due.len() {
            let mut level_ups = world.resource_mut::<LevelUps>();
            due.drain(..answered);
            due.append(&mut level_ups.0);
            mem::swap(&mut *due, &mut level_ups.0);
            return;
        }
    }
}

#[cfg(test)]
mod tests;
