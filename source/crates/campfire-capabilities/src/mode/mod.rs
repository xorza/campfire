//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

use std::any::Any;
use std::mem;
use std::rc::Rc;

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_script::rhai::ImmutableString;
use campfire_sim::{Breach, Capability, IdAllocator, SimSet, SimTick, StateRegistry, TickRate};

use crate::abilities::AbilitiesSet;
use crate::actions::ActionsSet;
use crate::combat::CombatSet;
use crate::combat::assist_window::AssistWindow;
use crate::combat::combat_hooks::CombatHooks;
use crate::combat::damage::DamageWeigher;
use crate::combat::heal::HealWeigher;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::mode::calls::Calls;
use crate::mode::choices::Choices;
use crate::mode::game_map::GameMap;
use crate::mode::match_end::MatchEnd;
use crate::mode::match_mode::MatchMode;
use crate::mode::mode_api::ModeApi;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_call::ModeCall;
use crate::mode::mode_hooks::ModeHooks;
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_state::ModeState;
use crate::mode::placed_unit::PlacedPath;
use crate::mode::save_asked::SaveAsked;
use crate::mode::timers::Timers;
use crate::mode::unanswered_deaths::UnansweredDeaths;
use crate::mode::unanswered_slot_events::UnansweredSlotEvents;
use crate::mode::unit_kit::kit_part::KitPart;
use crate::navigation::NavigationSet;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::orders::OrdersSet;
use crate::players::match_entity::MatchEntity;
use crate::players::player::Player;
use crate::players::player_resources::PlayerResources;
use crate::production::ProductionSet;
use crate::progression::track_book::TrackBook;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::state_value::StateValue;
use crate::stats::StatsSet;
use crate::units::UnitsSet;
use crate::units::delivery_entity::DeliveryEntity;
use crate::units::relations::Relations;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::team::Team;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_state::{SyncAll, SyncNone};
use crate::units::unit_type::UnitType;
use crate::units::view::View;
use crate::vision::Vision;

pub(crate) mod calls;
pub(crate) mod choice_book;
pub(crate) mod choice_data;
pub(crate) mod choices;
pub(crate) mod error;
pub(crate) mod game_map;
pub(crate) mod group_unit;
pub(crate) mod height_grid;
pub(crate) mod loadout_setup;
pub(crate) mod map_data;
pub(crate) mod map_ground;
pub(crate) mod map_point;
pub(crate) mod marker;
pub(crate) mod marker_spec;
pub(crate) mod match_end;
pub(crate) mod match_mode;
pub(crate) mod mode_api;
pub(crate) mod mode_book;
pub(crate) mod mode_books;
pub(crate) mod mode_call;
pub(crate) mod mode_data;
pub(crate) mod mode_effect;
pub(crate) mod mode_hooks;
pub(crate) mod mode_input;
pub(crate) mod mode_map;
pub(crate) mod mode_schema;
pub(crate) mod mode_setup;
pub(crate) mod mode_state;
pub(crate) mod mode_units;
pub(crate) mod offer;
pub(crate) mod placed_unit;
pub(crate) mod players_data;
pub(crate) mod region_data;
pub(crate) mod relation_data;
pub(crate) mod roster;
pub(crate) mod save_asked;
pub(crate) mod saves_data;
pub(crate) mod slot_action;
pub(crate) mod team_manifest;
pub(crate) mod timers;
pub(crate) mod unanswered_deaths;
pub(crate) mod unanswered_slot_events;
pub(crate) mod unit_kit;
pub(crate) mod unit_type_setup;

/// The mode of a match: the core's rules above the capabilities. Every match installs it after
/// its capabilities.
#[derive(Debug)]
pub struct Mode;

impl Declares for Mode {
    const MODULE: Option<Capability> = Some(Capability::Mode);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<MatchEnd>();
        d.component::<ModeState<SyncNone>>();
        d.component::<ModeState<SyncAll>>();
        d.resource::<Choices>();
        d.component::<PlayerResources>();
        d.component::<Player>();
        d.resource::<Timers>();
        d.resource::<UnansweredDeaths>();
        d.resource::<UnansweredSlotEvents>();
        d.check::<Player>(Mode::players_seated);
        d.part::<Player, PlayerResources>(|world| {
            PlayerResources::new(world.non_send::<View>().resource_count())
        });
        d.part::<MatchEntity, Relations>(|world| world.non_send::<MatchMode>().relations.clone());
        d.part::<MatchEntity, ModeState<SyncNone>>(|world| {
            world.non_send::<MatchMode>().state_none.clone()
        });
        d.part::<MatchEntity, ModeState<SyncAll>>(|world| {
            world.non_send::<MatchMode>().state_all.clone()
        });
        d.check::<UnitType>(Mode::kits_held);
        d.api(ModeApi::register);
    }
}

impl Mode {
    /// Adds the mode of `setup`, whose books the book builder built, to a match whose capabilities
    /// are installed and whose unit types, abilities and AI are loaded: in Inputs, the players'
    /// mode inputs run `on_mode_input`; in Mode, the tick's joins and leaves run `on_player_join`
    /// and `on_player_leave`, due timers run `on_timer`, the tick's deaths run `on_unit_died`, and
    /// the levels reached run `on_level_up`.
    /// The map's ground, paths and grid become the match's, and the mode's `[combat]` and
    /// `calc_damage` combat's.
    pub(crate) fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &StateRegistry,
        setup: ModeSetup<'_>,
        mut books: ModeBooks,
    ) {
        let view = world.non_send::<View>().clone();
        let rate = *world.resource::<TickRate>();
        let walkers = mem::take(&mut books.walkers);
        let loadout_ranks = books.loadout_ranks;
        // A window past what ticks can count covers the whole match.
        let assist_window = setup.data.combat.assist_window_ms.map(|ms| rate.window(ms));
        let ModeMap {
            ground,
            relations,
            paths,
            placed,
            markers,
            grid,
            brush,
        } = books.install(world);
        ground.install(world, walkers);
        let book = ModeBook::new(
            setup,
            world.resource::<ScriptBook>(),
            placed,
            relations,
            GameMap::new(paths.names().map(ImmutableString::from), &markers),
            loadout_ranks,
            KitPart::of_state(registry),
        );
        if let Some(grid) = grid {
            Vision::load_grid(world, grid, &brush, book.teams.count());
        }
        if let Some(window) = assist_window {
            world.insert_resource(AssistWindow(window));
        }
        view.set_names(Rc::clone(&book.teams), paths.shared_names());
        world.insert_resource(paths);
        let players = book.teams.players() as usize;
        world.insert_resource(book.choices.empty(players));
        world.insert_resource(Timers::default());
        world.insert_resource(UnansweredDeaths::default());
        world.insert_resource(UnansweredSlotEvents::default());
        let hooks = book.schema.hooks;
        let ctx = world.non_send::<Ctx>().clone();
        let book = Rc::new(book);
        ctx.frame().add_part(ModeCall::new(Rc::clone(&book)));
        ctx.set_mode(Rc::clone(&book) as Rc<dyn Any>);
        world.insert_non_send(MatchMode(Rc::clone(&book)));
        world.insert_non_send(Spawner::new(move |world, at, owner| {
            book.spawn_owned(world, at, owner)
        }));
        // A mode that defines either hook declares combat, as the load checks.
        if hooks.contains(Hook::CalcDamage) {
            let weigher = DamageWeigher::new(Calls::weigh_damage);
            world.non_send_mut::<CombatHooks>().weigher = Some(weigher);
        }
        if hooks.contains(Hook::CalcHeal) {
            let healer = HealWeigher::new(Calls::weigh_heal);
            world.non_send_mut::<CombatHooks>().healer = Some(healer);
        }
        schedule.add_systems((
            ModeHooks::mode_inputs
                .in_set(SimSet::Inputs)
                .after(UnitsSet::BeginTick)
                .after(StatsSet::Expire)
                .after(StatsSet::Regenerate)
                .after(NavigationSet::TrackStatics)
                .after(CombatSet::Respawn)
                .before(OrdersSet::Orders)
                .before(AbilitiesSet::Toggles)
                .before(ActionsSet::HoldAtInputs),
            (
                ModeHooks::slot_events,
                ModeHooks::run_timers,
                ModeHooks::unit_deaths,
                ModeHooks::level_ups,
            )
                .chain()
                .in_set(SimSet::Mode)
                .after(ProductionSet::Finish),
        ));
    }

    /// The boundary the mode in `world` last asked for a save at, by `ctx.save()`, by the tick
    /// it comes before.
    pub fn save_asked(world: &World) -> Option<Tick> {
        world.get_resource::<SaveAsked>().map(|asked| asked.at())
    }

    /// The mode's state in the match in `world`, every field in the order of their names, as a
    /// mode call reads it.
    pub fn state(world: &World) -> Vec<StateValue> {
        let mut state = Vec::new();
        let (none, all) = (
            ModeState::<SyncNone>::of(world),
            ModeState::<SyncAll>::of(world),
        );
        world
            .non_send::<MatchMode>()
            .schema
            .merge_state(none, all, &mut state);
        state
    }

    /// Whether each slot of the session has its entity, as the mode's start spawns them, so no
    /// call finds one missing.
    fn players_seated(world: &World) -> Result<(), Breach> {
        let players = world.non_send::<MatchMode>().teams.players();
        let seated = (0..players)
            .map(PlayerSlot::new)
            .all(|slot| Player::entity(world, slot).is_some());
        Breach::of_match(seated)
    }

    /// Whether each unit but a delivery holds every part its type's kit gives it as it spawns,
    /// none of which a unit loses while it stands, so no system finds one missing; the first
    /// that lacks one. A match with no progression has no level track, as `ModeBook::spawn` reads
    /// it.
    fn kits_held(world: &World) -> Result<(), Breach> {
        let book = world.non_send::<MatchMode>();
        let level_track = world
            .get_resource::<TrackBook>()
            .and_then(TrackBook::level_track);
        Breach::first(world, |entity| {
            let unit = world.entity(entity);
            unit.contains::<UnitEntity>()
                && !unit.contains::<DeliveryEntity>()
                && !book.holds_kit(unit, level_track)
        })
    }

    /// The team of player `slot` in the match in `world`; `None` before the mode installs, or for
    /// a slot the session does not have.
    pub fn team_of(world: &World, slot: PlayerSlot) -> Option<Team> {
        world.get_non_send::<MatchMode>()?.teams.of(slot)
    }

    /// Starts the match, before its first tick: the map's placed units spawn, on their paths and
    /// walking them as they name, then `on_match_start` runs. A timer it sets counts from the
    /// start. An error when the call fails: a match its mode cannot start would run without its
    /// rules.
    pub fn start(world: &mut World) -> Result<(), CallError> {
        let book = world.non_send::<MatchMode>().clone();
        let teams = &book.teams;
        let slots = (0..teams.players()).map(PlayerSlot::new);
        MatchEntity::spawn(
            world,
            slots.map(|slot| teams.of(slot).expect("every player has a team")),
        );
        for placed in &book.placed {
            let at = SpawnAt {
                id: world.resource_mut::<IdAllocator>().allocate(),
                unit_type: placed.unit_type,
                team: placed.team,
                pos: placed.pos,
                angle: placed.angle,
            };
            match placed.path {
                None => book.spawn(world, at, None, ()),
                Some(PlacedPath { path, from: None }) => {
                    book.spawn(world, at, None, OnPath::new(path))
                }
                Some(PlacedPath {
                    path,
                    from: Some(from),
                }) => {
                    let walker = (OnPath::new(path), PathWalker::start(from, at.id));
                    book.spawn(world, at, None, walker)
                }
            };
        }
        if !book.schema.hooks.contains(Hook::OnMatchStart) {
            return Ok(());
        }
        let now = world.resource::<SimTick>().start();
        Calls::batch(world, now, |call| {
            call.run(Pool::Mode, Hook::OnMatchStart, (call.ctx.clone(),))
        })
        .map_err(CallError::from_script)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position, StableId};

    use crate::mode::match_mode::MatchMode;
    use crate::scripts::ctx::Ctx;
    use crate::units::spawn_at::SpawnAt;
    use crate::units::spawner::Spawner;
    use crate::units::team::Team;

    /// Spawns a unit of the mode's type `name` on `team` at `pos`, with the parts its type's
    /// kit gives, as the mode's own spawns do.
    pub fn spawn_typed(world: &mut World, name: &str, team: Team, pos: Position) -> StableId {
        let ctx = world.non_send::<Ctx>().clone();
        let book = world.non_send::<MatchMode>().clone();
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
