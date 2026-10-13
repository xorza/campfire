//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

use std::rc::Rc;

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_script::rhai::ImmutableString;
use campfire_sim::{Breach, Capability, IdAllocator, SimPlace, SimSet, SimTick};

use crate::abilities::AbilitiesSet;
use crate::actions::ActionsSet;
use crate::actions::action_slots::ActionSlots;
use crate::combat::CombatSet;
use crate::combat::combat_hooks::CombatHooks;
use crate::combat::damage::DamageWeigher;
use crate::combat::heal::HealWeigher;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::set_order::SetOrder;
use crate::mode::calls::Calls;
use crate::mode::choices::Choices;
use crate::mode::game_map::GameMap;
use crate::mode::match_end::MatchEnd;
use crate::mode::match_mode::MatchMode;
use crate::mode::mode_api::ModeApi;
use crate::mode::mode_call::ModeCall;
use crate::mode::mode_hooks::ModeHooks;
use crate::mode::mode_load::ModeLoad;
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_setup::ModeSetup;
use crate::mode::mode_spec::{MapStart, ModeSpec};
use crate::mode::mode_state::ModeState;
use crate::mode::mode_units::ModeUnits;
use crate::mode::mode_world::ModeWorldCrate;
use crate::mode::placed_unit::PlacedPath;
use crate::mode::save_asked::SaveAsked;
use crate::mode::timers::Timers;
use crate::mode::unanswered_deaths::UnansweredDeaths;
use crate::mode::unanswered_slot_events::UnansweredSlotEvents;
use crate::navigation::NavigationSet;
use crate::navigation::navigation_book::NavigationBook;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::orders::OrdersSet;
use crate::players::Players;
use crate::players::match_entity::MatchEntity;
use crate::players::out_of_play::OutOfPlay;
use crate::players::player::Player;
use crate::players::players_world::PlayersWorld;
use crate::production::ProductionSet;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_book::ScriptBook;
use crate::stats::StatsSet;
use crate::units::UnitsSet;
use crate::units::body_form::BodyForm;
use crate::units::by_type::ByType;
use crate::units::relations::Relations;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::unit_state::{SyncAll, SyncNone};
use crate::units::view::View;

pub(crate) mod calls;
pub(crate) mod choice_data;
pub(crate) mod choice_id;
pub(crate) mod choice_specs;
pub(crate) mod choices;
pub(crate) mod error;
pub(crate) mod game_map;
pub(crate) mod height_grid;
pub(crate) mod loadout_setup;
pub(crate) mod map_data;
pub(crate) mod map_point;
pub(crate) mod marker;
pub(crate) mod marker_spec;
pub(crate) mod marker_tag;
pub(crate) mod match_end;
pub(crate) mod match_mode;
pub(crate) mod mode_api;
pub(crate) mod mode_call;
pub(crate) mod mode_data;
pub(crate) mod mode_effect;
pub(crate) mod mode_hooks;
pub(crate) mod mode_input;
pub(crate) mod mode_inputs;
pub(crate) mod mode_load;
pub(crate) mod mode_map;
pub(crate) mod mode_names;
pub(crate) mod mode_schema;
pub(crate) mod mode_setup;
pub(crate) mod mode_spec;
pub(crate) mod mode_state;
pub(crate) mod mode_units;
pub(crate) mod mode_world;
pub(crate) mod offer;
pub(crate) mod placed_unit;
pub(crate) mod players_data;
pub(crate) mod region_data;
pub(crate) mod relation_data;
pub(crate) mod roster;
pub(crate) mod save_asked;
pub(crate) mod saves_data;
pub(crate) mod spawn_group;
pub(crate) mod team_manifest;
pub(crate) mod timers;
pub(crate) mod unanswered_deaths;
pub(crate) mod unanswered_slot_events;
pub(crate) mod unit_type_setup;

/// The mode of a match: the core's rules above the capabilities. Every match installs it after
/// its capabilities.
#[derive(Debug)]
pub struct Mode;

campfire_common::enum_all! {
    /// The mode's steps.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum ModeSet {
        /// The tick's mode inputs run its script, once the tick started and before any order.
        Inputs,
        /// Once the trains spawned: the slots' events, the due timers, the deaths and the level
        /// ups run the mode's hooks.
        Hooks,
        /// The players out of play are counted again.
        OutOfPlay,
    }
    /// Every step.
    pub(crate) const ALL;
}

impl ModuleSet for ModeSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const ALL: &'static [ModeSet] = &ModeSet::ALL;

    fn place(self) -> SimPlace {
        SimPlace::Stage(match self {
            ModeSet::Inputs => SimSet::Inputs,
            ModeSet::Hooks => SimSet::Mode,
            ModeSet::OutOfPlay => SimSet::Vision,
        })
    }

    fn order(self, o: &mut SetOrder) {
        match self {
            ModeSet::Inputs => {
                o.after(UnitsSet::BeginTick)
                    .after(StatsSet::Expire)
                    .after(StatsSet::Regenerate)
                    .after(NavigationSet::TrackStatics)
                    .after(CombatSet::Respawn)
                    .before(OrdersSet::Orders)
                    .before(AbilitiesSet::Toggles)
                    .before(ActionsSet::HoldAtInputs);
            }
            ModeSet::Hooks => {
                o.after(ProductionSet::Finish);
            }
            ModeSet::OutOfPlay => {}
        }
    }
}

impl Module for Mode {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const NEEDS: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<ModeSet>();
        d.component::<MatchEnd>();
        d.component::<ModeState<SyncNone>>();
        d.component::<ModeState<SyncAll>>();
        d.resource::<Choices>();
        d.resource::<Timers>();
        d.resource::<UnansweredDeaths>();
        d.resource::<UnansweredSlotEvents>();
        d.report::<SaveAsked>();
        d.check::<Player>(Mode::players_seated);
        d.part::<MatchEntity, Relations>(|at| {
            Some(at.world().non_send::<MatchMode>().relations.clone())
        });
        d.part::<MatchEntity, ModeState<SyncNone>>(|at| {
            Some(at.world().non_send::<MatchMode>().state_none.clone())
        });
        d.part::<MatchEntity, ModeState<SyncAll>>(|at| {
            Some(at.world().non_send::<MatchMode>().state_all.clone())
        });
        d.api(ModeApi::register);
    }

    /// Adds the mode's hooks to a match: in Inputs, the players' mode inputs run
    /// `on_mode_input`; in Mode, the tick's joins and leaves run `on_player_join` and
    /// `on_player_leave`, due timers run `on_timer`, the tick's deaths run `on_unit_died`, and the
    /// levels reached run `on_level_up`. `Mode::load` gives them the mode they run.
    fn install(cx: &mut InstallCx<'_>) {
        cx.schedule.add_systems((
            ModeHooks::mode_inputs.in_set(ModeSet::Inputs),
            (
                ModeHooks::slot_events,
                ModeHooks::run_timers,
                ModeHooks::unit_deaths,
                ModeHooks::level_ups,
            )
                .chain()
                .in_set(ModeSet::Hooks),
            OutOfPlay::refresh.in_set(ModeSet::OutOfPlay),
        ));
    }
}

impl Mode {
    /// Loads the mode of `setup` into a match whose capabilities, mode and books are installed
    /// and whose unit types, abilities and AI are loaded, with what `load` takes from the books:
    /// the map's pathing grid, for the kinds of its walkers, and its teams.
    pub(crate) fn load(world: &mut World, setup: ModeSetup<'_>, load: ModeLoad) {
        let view = world.non_send::<View>().clone();
        let ModeLoad {
            map,
            walkers,
            loadout_ranks,
        } = load;
        map.install_pathing(world, walkers);
        let ModeMap {
            teams,
            relations,
            placed,
            markers,
            ..
        } = map;
        let paths = world
            .get_resource::<NavigationBook>()
            .map(|book| &book.paths);
        let path_names = paths.into_iter().flat_map(|paths| paths.names().texts());
        let start = MapStart {
            teams,
            placed,
            relations,
            map: GameMap::new(path_names.map(ImmutableString::from), &markers),
        };
        Mode::load_units(world, &setup.units);
        let book = ModeSpec::new(
            setup,
            world.resource::<ScriptBook>(),
            start,
            loadout_ranks,
            world.resource::<ByType<BodyForm>>(),
            world.resource::<ByType<ActionSlots>>(),
        );
        view.names_mut().set_teams(Rc::clone(&book.teams));
        let players = book.teams.players() as usize;
        world.insert_resource(book.choices.empty(players));
        let hooks = book.schema.hooks;
        let ctx = world.non_send::<Ctx>().clone();
        let book = Rc::new(book);
        ctx.frame().add_part(ModeCall::new(Rc::clone(&book)));
        ctx.set_mode(Rc::<ModeSpec>::clone(&book));
        world.insert_non_send(MatchMode(Rc::clone(&book)));
        // A mode that defines either hook declares combat, as the load checks.
        if hooks.contains(Hook::CalcDamage) {
            let weigher = DamageWeigher::new(Calls::weigh_damage);
            world.non_send_mut::<CombatHooks>().weigher = Some(weigher);
        }
        if hooks.contains(Hook::CalcHeal) {
            let healer = HealWeigher::new(Calls::weigh_heal);
            world.non_send_mut::<CombatHooks>().healer = Some(healer);
        }
    }

    /// Whether each slot of the session has its entity, as the mode's start spawns them, so no
    /// call finds one missing.
    fn players_seated(world: &World) -> Result<(), Breach> {
        let players = world.non_send::<MatchMode>().teams.players();
        let seated = (0..players)
            .map(PlayerSlot::new)
            .all(|slot| world.player_entity(slot).is_some());
        Breach::of_match(seated)
    }

    /// Loads the spawner of `units`, the mode's unit types, into `world`: from then on, the mode
    /// and each capability spawn a unit of a type the mode spawns through it.
    pub fn load_units(world: &mut World, units: &ModeUnits) {
        world.insert_non_send(units.spawner());
    }

    /// Starts the match, before its first tick: the map's placed units spawn, on their paths and
    /// walking them as they name, then `on_match_start` runs. A timer it sets counts from the
    /// start. An error when the call fails: a match its mode cannot start would run without its
    /// rules.
    pub fn start(world: &mut World) -> Result<(), CallError> {
        let book = world.non_send::<MatchMode>().clone();
        let teams = &book.teams;
        let slots = (0..teams.players()).map(PlayerSlot::new);
        Players::spawn_match(
            world,
            slots.map(|slot| teams.of(slot).expect("every player has a team")),
        );
        let spawner = world.non_send::<Spawner>().clone();
        for placed in &book.placed {
            let at = SpawnAt {
                id: world.resource_mut::<IdAllocator>().allocate(),
                unit_type: placed.unit_type,
                team: placed.team,
                pos: placed.pos,
                owner: None,
                angle: placed.angle,
            };
            match placed.path {
                None => spawner.spawn(world, at, ()),
                Some(PlacedPath { path, from: None }) => {
                    spawner.spawn(world, at, OnPath::new(path))
                }
                Some(PlacedPath {
                    path,
                    from: Some(from),
                }) => {
                    let walker = (OnPath::new(path), PathWalker::start(from, at.id));
                    spawner.spawn(world, at, walker)
                }
            };
        }
        if !book.schema.hooks.contains(Hook::OnMatchStart) {
            return Ok(());
        }
        let now = world.resource::<SimTick>().start();
        world
            .mode_calls(now, |call| {
                call.run(Pool::Mode, Hook::OnMatchStart, (call.ctx.clone(),))
            })
            .map_err(CallError::from_script)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position, StableId};

    use crate::mode::Mode;
    use crate::units::Units;
    use crate::units::spawn_at::SpawnAt;
    use crate::units::spawner::Spawner;
    use crate::units::team::Team;

    impl Mode {
        /// Spawns a unit of the mode's type `name` at `pos` on `team`, of the next stable id, with
        /// `parts`, as the mode's own spawns do.
        pub fn spawn_typed(
            world: &mut World,
            name: &str,
            pos: Position,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let unit_type =
                Units::type_named(world, name).unwrap_or_else(|| panic!("no unit type {name}"));
            let id = world.resource_mut::<IdAllocator>().allocate();
            let at = SpawnAt {
                id,
                unit_type,
                team,
                owner: None,
                pos,
                angle: Num::ZERO,
            };
            let spawner = world.non_send::<Spawner>().clone();
            spawner.spawn(world, at, parts);
            id
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::slice;

    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_script::rhai::Dynamic;
    use campfire_script::{Budget, ScriptHost};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{
        EntityIndex, Position, SimTick, SimWorld, SlotEvent, SlotEventKind, StableId, TickInputs,
    };

    use crate::actions::Actions;
    use crate::combat::damage::{Damage, DamageCause};
    use crate::combat::pass_queue::PassQueue;
    use crate::mode::Mode;
    use crate::mode::mode_input::{InputValue, ModeInput};
    use crate::mode::mode_spec::ModeSpec;
    use crate::mode::mode_world::ModeWorld;
    use crate::navigation::path_walker::{PathEnd, PathWalker};
    use crate::scripts::call_start::CallStart;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::scripts::error::internals::FailureKind;
    use crate::scripts::script_failures::ScriptFailures;
    use crate::scripts::script_role::ScriptRole;
    use crate::scripts::state_value::StateValue;
    use crate::stats::Stats;
    use crate::stats::application::{Application, NewInstance};
    use crate::stats::instance::StatShare;
    use crate::stats::level::Level;
    use crate::stats::life::Life;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_op::StatOp;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::Units;
    use crate::units::owner::Owner;
    use crate::units::tag_set::TagSet;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::rank::Rank;
    use crate::values::stat::Stat;

    impl Mode {
        /// Player `slot` picks `hero` in a tick: the hero it then owns.
        pub(crate) fn pick(world: &mut World, slot: u32, hero: &str) -> Entity {
            let input = ModeInput {
                name: "hero",
                value: InputValue::String(hero),
            };
            Mode::tick_inputs(world, &[(slot, input)]);
            let player = PlayerSlot::new(slot);
            let mut owned = world.query::<(Entity, &Owner)>();
            let mut heroes = owned
                .iter(world)
                .filter(|(_, owner)| owner.slot() == player);
            let (hero, _) = heroes.next().expect("the player picked a hero");
            assert!(heroes.next().is_none(), "the player owns one hero");
            hero
        }

        /// Runs a tick with `inputs`, each a player's slot and a mode input.
        pub(crate) fn tick_inputs(world: &mut World, inputs: &[(u32, ModeInput<'_>)]) {
            for (slot, input) in inputs {
                let payload = ModeInput::payload(slice::from_ref(input));
                world.push_input(PlayerSlot::new(*slot), &payload);
            }
            world.step();
        }

        /// Runs a tick in which the session logged `events`, each a player's join or leave.
        pub(crate) fn tick_slots(world: &mut World, events: &[(u32, SlotEventKind)]) {
            for &(slot, kind) in events {
                let slot = PlayerSlot::new(slot);
                let mut inputs = world.resource_mut::<TickInputs>();
                inputs.push_slot_event(SlotEvent { slot, kind });
            }
            world.step();
        }

        /// Unit `id`.
        pub(crate) fn entity_by_id(world: &World, id: u64) -> Entity {
            let mut units = world.resource::<EntityIndex>().iter();
            units.find(|(unit, _)| unit.get() == id).unwrap().1
        }

        /// The state fields `phase`, `seen`, `count` and `inputs`.
        pub(crate) fn tracked_state(world: &World) -> [StateValue; 4] {
            ["phase", "seen", "count", "inputs"].map(|name| Mode::field(world, name))
        }

        /// The state field `name`: the state holds the fields in the order of their names.
        pub(crate) fn field(world: &World, name: &str) -> StateValue {
            let ctx = world.non_send::<Ctx>();
            let at = ModeSpec::of(ctx)
                .unwrap()
                .schema
                .state_field(name)
                .unwrap()
                .index;
            world.mode_state()[at].clone()
        }

        /// Each unit: its id, where it stands, its team, and the end of a path it walks from.
        pub(crate) fn placed_units(world: &World) -> Vec<(u64, Position, usize, Option<PathEnd>)> {
            world
                .resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| world.entity(entity).contains::<UnitType>())
                .map(|(id, entity)| {
                    let unit = world.entity(entity);
                    (
                        id.get(),
                        *unit.get::<Position>().unwrap(),
                        unit.get::<Team>().unwrap().index(),
                        unit.get::<PathWalker>().map(|walker| walker.walks_from()),
                    )
                })
                .collect()
        }

        /// The kind of each of the tick's failed calls.
        pub(crate) fn failures(world: &World) -> Vec<FailureKind> {
            let failures = world.non_send::<ScriptFailures>().calls();
            failures.into_iter().map(|failure| failure.kind).collect()
        }

        /// A grunt of 1000 health on `team`, whose modifier adds `stats` by name.
        pub(crate) fn stat_grunt(world: &mut World, team: Team, stats: &[(&str, Num)]) -> StableId {
            let book = world.resource::<StatBook>();
            let changes: Vec<_> = stats
                .iter()
                .map(|&(name, _)| {
                    (
                        book.named(&Stat::named(name).unwrap()).unwrap(),
                        StatOp::Add,
                    )
                })
                .collect();
            let id = Stats::push_modifier_changes(world, &changes, TagSet::default());
            let shares = stats
                .iter()
                .map(|&(_, value)| StatShare { value, live: None });
            let instance = NewInstance {
                stats: shares.collect(),
                ..NewInstance::bare(id, None)
            };
            let modifiers = Modifiers::bundle([Application::refresh(instance)]);
            let grunt = Units::type_named(world, "grunt").unwrap();
            let parts = (
                Life::full(Num::int(1000)),
                Level::default(),
                UnitStats::default(),
                modifiers,
            );
            Units::spawn_of(world, grunt, Position::ORIGIN, team, parts)
        }

        /// Queues `amount` of damage of the kind `kind` from `source` to `target`, dealt by
        /// `cause`.
        pub(crate) fn damage_of_kind(
            world: &mut World,
            source: Option<StableId>,
            target: StableId,
            amount: i64,
            kind: &str,
            cause: DamageCause,
        ) {
            let kind = world
                .non_send::<View>()
                .names()
                .damage_kinds
                .named(kind)
                .expect("the mode names the damage kind");
            world.resource_mut::<PassQueue>().push_damage(Damage {
                source,
                target,
                amount: Num::int(amount),
                kind,
                cause,
                action: None,
                depth: 0,
                hit: None,
            });
        }

        /// Runs `probe(ctx, unit)` of `source` as a call of `role`, acting as `actor` but in the
        /// mode's, then applies its effects: what it returned, or why it failed. An action's call
        /// is the mode's Strike's, a modifier's its Blessing's.
        pub(crate) fn probe_as(
            world: &mut World,
            source: &str,
            role: ScriptRole,
            actor: StableId,
            id: StableId,
        ) -> Result<Dynamic, CallError> {
            let ctx = world.non_send::<Ctx>().clone();
            ctx.view().read(world);
            let now = world.resource::<SimTick>().start();
            match role {
                ScriptRole::Mode => ctx.frame().begin_mode(world, false, now),
                ScriptRole::Ai => ctx.frame().begin_think(world, actor, now),
                ScriptRole::Action => {
                    let strike = Actions::named(world, 0, "strike").unwrap();
                    let start = CallStart::cast(strike, Rank::FIRST, actor, 0, now);
                    ctx.frame().begin(world, start).unwrap();
                }
                ScriptRole::Modifier => {
                    let blessing = Stats::modifier(world, 0, "blessing").unwrap();
                    let start = CallStart {
                        acting: Some(actor),
                        ..CallStart::hook(blessing, 0, 1, now)
                    };
                    ctx.frame().begin(world, start).unwrap();
                }
            }
            let handle = ctx.view().unit(id).unwrap();
            let returned = {
                let mut host = world.non_send_mut::<ScriptHost>();
                let script = host.compile(source).unwrap();
                let mut budget = Budget::new(u64::MAX);
                host.call(&mut budget, script, "probe", (ctx.clone(), handle))
            };
            let returned = returned.map_err(CallError::from_script)?;
            ctx.apply(world);
            Ok(returned)
        }
    }
}

#[cfg(test)]
mod tests;
