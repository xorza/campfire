//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::schedule::common_conditions::not;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemCondition, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::ActionSlots;
use campfire_combat::{CombatHooks, DamageWeigher, HealWeigher};
use campfire_common::PlayerSlot;
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_navigation::{NavigationBook, OnPath, PathWalker};
use campfire_players::{OutOfPlay, Players, PlayersWorld};
use campfire_script::{CallError, Ctx, Hook, Pool, ScriptText, View};
use campfire_sim::{Breach, Capability, IdAllocator, SimTick};
use campfire_units::{
    BodyForm, MatchEntity, Player, ScriptBook, SpawnAt, Spawner, SyncAll, SyncNone,
};
use campfire_values::Relations;

mod calls;
mod choice_data;
mod choice_id;
mod choice_specs;
mod choices;
mod error;
mod game_map;
mod height_grid;
mod loadout_setup;
mod map_data;
mod map_point;
mod marker;
mod marker_spec;
mod marker_tag;
mod match_end;
mod match_mode;
mod mode_api;
mod mode_call;
mod mode_data;
mod mode_effect;
mod mode_hooks;
mod mode_input;
mod mode_inputs;
mod mode_load;
mod mode_map;
mod mode_names;
mod mode_schema;
mod mode_setup;
mod mode_spec;
mod mode_state;
mod mode_units;
mod mode_world;
mod offer;
mod placed_unit;
mod players_data;
mod region_data;
mod relation_data;
mod roster;
mod save_asked;
mod saves_data;
mod spawn_group;
mod team_data;
mod team_manifest;
mod timers;
mod unanswered_deaths;
mod unanswered_slot_events;
mod unit_type_setup;

pub use crate::calls::Calls;
pub use crate::choice_data::{ChoiceData, Offers};
pub use crate::choices::Choices;
pub use crate::error::{ModeError, ModeList, ModeNamesError};
pub use crate::game_map::GameMap;
pub use crate::height_grid::HeightGrid;
pub use crate::loadout_setup::LoadoutSetup;
pub use crate::map_data::{
    BrushData, GridData, MapData, MapNavigationData, MarkerData, PathData, PlacedUnitData, WallData,
};
pub use crate::map_point::MapPoint;
pub use crate::marker::{Marker, MarkerInfo};
pub use crate::marker_spec::MarkerSpec;
pub use crate::match_end::{MatchEnd, MatchResult};
pub use crate::match_mode::MatchMode;
pub use crate::mode_api::ModeApi;
pub use crate::mode_call::ModeCall;
pub use crate::mode_data::{InputType, ListEntry, ModeData, ModeParam};
pub use crate::mode_hooks::ModeHooks;
pub use crate::mode_input::{InputValue, ModeInput};
pub use crate::mode_inputs::ModeInputs;
pub use crate::mode_load::ModeLoad;
pub use crate::mode_map::{ModeMap, ResolvedMap, WalkCheck};
pub use crate::mode_names::ModeNames;
pub use crate::mode_setup::ModeSetup;
pub use crate::mode_spec::{MapStart, ModeSpec};
pub use crate::mode_state::{ModeState, ModeSync};
pub use crate::mode_units::ModeUnits;
pub use crate::mode_world::ModeWorld;
pub use crate::offer::Offer;
pub use crate::placed_unit::{PlacedPath, PlacedUnit};
pub use crate::players_data::{Leaver, PlayersData};
pub use crate::region_data::RegionData;
pub use crate::relation_data::RelationData;
pub use crate::save_asked::SaveAsked;
pub use crate::saves_data::{SaveBy, SavesData};
pub use crate::team_data::TeamData;
pub use crate::team_manifest::TeamManifest;
pub use crate::timers::{Timer, Timers};
pub use crate::unanswered_deaths::{DeathCall, UnansweredDeaths};
pub use crate::unanswered_slot_events::UnansweredSlotEvents;
pub use crate::unit_type_setup::UnitTypeSetup;

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
    pub const ALL;
}

impl ModuleSet for ModeSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const ALL: &'static [ModeSet] = &ModeSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            ModeSet::Inputs => Phase::ModeInputs,
            ModeSet::Hooks => Phase::ModeHooks,
            ModeSet::OutOfPlay => Phase::End,
        })
    }
}

impl Module for Mode {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[
        Capability::Combat,
        Capability::Navigation,
        Capability::Production,
        Capability::Progression,
        Capability::Stats,
        Capability::Vision,
    ];

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
            Some(ModeState::initial(
                &at.world().non_send::<MatchMode>().schema,
            ))
        });
        d.part::<MatchEntity, ModeState<SyncAll>>(|at| {
            Some(ModeState::initial(
                &at.world().non_send::<MatchMode>().schema,
            ))
        });
        d.api(ModeApi::register);
    }

    /// Adds the mode's hooks to a match: in Inputs, the players' mode inputs run
    /// `on_mode_input`; in Mode, the tick's joins and leaves run `on_player_join` and
    /// `on_player_leave`, due timers run `on_timer`, or fire with no call in a mode without it, the
    /// tick's deaths run `on_unit_died`, and, with progression, the levels reached run
    /// `on_level_up`, or clear in a mode without it. `Mode::load` gives them the mode they run,
    /// and each hook's system runs only in a mode that defines its hook.
    fn install(cx: &mut InstallCx<'_>) {
        let defines = ModeHooks::defines;
        let joins = defines(Hook::OnPlayerJoin).or_else(defines(Hook::OnPlayerLeave));
        cx.schedule.add_systems((
            ModeHooks::mode_inputs
                .run_if(defines(Hook::OnModeInput))
                .in_set(ModeSet::Inputs),
            (
                ModeHooks::slot_events.run_if(joins),
                ModeHooks::run_timers.run_if(defines(Hook::OnTimer)),
                ModeHooks::fire_timers.run_if(not(defines(Hook::OnTimer))),
                ModeHooks::unit_deaths.run_if(defines(Hook::OnUnitDied)),
            )
                .chain()
                .in_set(ModeSet::Hooks),
            OutOfPlay::refresh.in_set(ModeSet::OutOfPlay),
        ));
        if cx.has(Capability::Progression) {
            cx.schedule.add_systems(
                (
                    ModeHooks::level_ups.run_if(defines(Hook::OnLevelUp)),
                    ModeHooks::clear_level_ups.run_if(not(defines(Hook::OnLevelUp))),
                )
                    .chain()
                    .after(ModeHooks::unit_deaths)
                    .in_set(ModeSet::Hooks),
            );
        }
    }
}

impl Mode {
    /// Loads the mode of `setup` into a match whose capabilities, mode and books are installed
    /// and whose unit types, abilities and AI are loaded, with what `load` takes from the books:
    /// the map's pathing grid, for the kinds of its walkers, and its teams.
    pub fn load(world: &mut World, setup: ModeSetup<'_>, load: ModeLoad) {
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
            map: GameMap::new(path_names.map(ScriptText::from), &markers),
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
        view.names_mut().set_table(Arc::clone(book.choices.names()));
        let players = book.teams.players() as usize;
        world.insert_resource(Choices::empty(&book.choices, players));
        let hooks = book.schema.hooks;
        let ctx = world.non_send::<Ctx>().clone();
        let book = Rc::new(book);
        ctx.frame().add_params(ModeCall::new(Rc::clone(&book)));
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
        Calls::scope(world, now, |call| {
            call.run(Pool::Mode, Hook::OnMatchStart, (call.ctx.clone(),))
        })
        .map_err(CallError::from_script)
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::slice;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_actions::Actions;
    use campfire_combat::{Damage, DamageCause, PassQueue};
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_navigation::{PathEnd, PathWalker};
    use campfire_script::internals::FailureKind;
    use campfire_script::{
        CallError, CallStart, Ctx, ScriptFailures, ScriptRole, ScriptValue, StateValue, View,
    };
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{
        EntityIndex, IdAllocator, Position, SimTick, SimWorld, SlotEvent, SlotEventKind, StableId,
        TickInputs,
    };
    use campfire_stats::{
        Application, Life, Modifiers, NewInstance, StatBook, StatOp, StatShare, Stats,
    };
    use campfire_units::{Owner, SpawnAt, Spawner, UnitTypeBook, Units};
    use campfire_values::{Rank, Stat, TagSet, Team, UnitType};

    use crate::Mode;
    use crate::mode_input::{InputValue, ModeInput};
    use crate::mode_spec::ModeSpec;
    use crate::mode_units::ModeUnits;
    use crate::mode_world::ModeWorld;
    use crate::unit_type_setup::UnitTypeSetup;

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
            Mode::spawn_at(world, unit_type, pos, Num::ZERO, team, parts)
        }

        /// Spawns a unit of the mode's type `unit_type` at `pos`, turned `angle` degrees, on
        /// `team`, of the next stable id, with `parts`, through the match's spawner, as the mode's
        /// own spawns do.
        pub(super) fn spawn_at(
            world: &mut World,
            unit_type: UnitType,
            pos: Position,
            angle: Num,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let id = world.resource_mut::<IdAllocator>().allocate();
            let at = SpawnAt {
                id,
                unit_type,
                team,
                owner: None,
                pos,
                angle,
            };
            let spawner = world
                .get_non_send::<Spawner>()
                .expect("the mode loaded its unit types before a unit spawns")
                .clone();
            spawner.spawn(world, at, parts);
            id
        }
    }

    impl Mode {
        /// Spawns a unit of the mode's type `unit_type` at `pos` on `team`, of the next stable
        /// id, with `parts`, as the mode's own spawns do, once the match loaded its types: the
        /// first spawn seals their tags, with no effect for a tag, as a mode's load does once
        /// its packages load, and loads the spawner of `load_test_units` when the test loaded
        /// none.
        pub fn spawn(
            world: &mut World,
            unit_type: UnitType,
            pos: impl Into<Position>,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            Mode::spawn_turned(world, unit_type, pos, Num::ZERO, team, parts)
        }

        /// Spawns a unit as `spawn` does, turned `angle` degrees.
        pub fn spawn_turned(
            world: &mut World,
            unit_type: UnitType,
            pos: impl Into<Position>,
            angle: Num,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            Units::seal_once(world);
            if world.get_non_send::<Spawner>().is_none() {
                Mode::load_test_units(world);
            }
            Mode::spawn_at(world, unit_type, pos.into(), angle, team, parts)
        }

        /// Loads the spawner of every unit type the match loaded so far, which applies no
        /// passive, as a mode's load gives its own: a fixture that loads types loads it again.
        pub fn load_test_units(world: &mut World) {
            let count = world.resource::<UnitTypeBook>().count();
            let types = (0..count).map(|index| UnitTypeSetup {
                unit_type: UnitType::new(u16::try_from(index).expect("unit types fit u16")),
                passive: None,
            });
            let units = ModeUnits {
                unit_types: types.collect(),
                ..ModeUnits::default()
            };
            Mode::load_units(world, &units);
        }

        /// Spawns a unit of the mode's type `name`, as `spawn` does.
        pub fn spawn_named(
            world: &mut World,
            name: &str,
            pos: impl Into<Position>,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let unit_type =
                Units::type_named(world, name).unwrap_or_else(|| panic!("no unit type {name}"));
            Mode::spawn(world, unit_type, pos, team, parts)
        }

        /// Player `slot` picks `hero` in a tick: the hero it then owns.
        pub fn pick(world: &mut World, slot: u32, hero: &str) -> Entity {
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
        pub fn tick_inputs(world: &mut World, inputs: &[(u32, ModeInput<'_>)]) {
            for (slot, input) in inputs {
                let payload = ModeInput::payload(slice::from_ref(input));
                world.push_input(PlayerSlot::new(*slot), &payload);
            }
            world.step();
        }

        /// Runs a tick in which the session logged `events`, each a player's join or leave.
        pub fn tick_slots(world: &mut World, events: &[(u32, SlotEventKind)]) {
            for &(slot, kind) in events {
                let slot = PlayerSlot::new(slot);
                let mut inputs = world.resource_mut::<TickInputs>();
                inputs.push_slot_event(SlotEvent { slot, kind });
            }
            world.step();
        }

        /// Unit `id`.
        pub fn entity_by_id(world: &World, id: u64) -> Entity {
            let mut units = world.resource::<EntityIndex>().iter();
            units.find(|(unit, _)| unit.get() == id).unwrap().1
        }

        /// The state fields `phase`, `seen`, `count` and `inputs`.
        pub fn tracked_state(world: &World) -> [StateValue; 4] {
            ["phase", "seen", "count", "inputs"].map(|name| Mode::field(world, name))
        }

        /// The state field `name`: the state holds the fields in the order of their names.
        pub fn field(world: &World, name: &str) -> StateValue {
            let ctx = world.non_send::<Ctx>();
            let at = ModeSpec::of(ctx)
                .unwrap()
                .schema
                .state
                .field(name)
                .unwrap()
                .index();
            world.mode_state()[at].clone()
        }

        /// Each unit: its id, where it stands, its team, and the end of a path it walks from.
        pub fn placed_units(world: &World) -> Vec<(u64, Position, usize, Option<PathEnd>)> {
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
        pub fn failures(world: &World) -> Vec<FailureKind> {
            let failures = world.non_send::<ScriptFailures>().calls();
            failures.into_iter().map(|failure| failure.kind).collect()
        }

        /// A grunt of the mode's on `team`, of 1000 health in place of its type's, whose modifier
        /// adds `stats` by name.
        pub fn stat_grunt(world: &mut World, team: Team, stats: &[(&str, Num)]) -> StableId {
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
            let parts = (Life::full(Num::int(1000)), modifiers);
            Mode::spawn(world, grunt, Position::ORIGIN, team, parts)
        }

        /// Queues `amount` of damage of the kind `kind` from `source` to `target`, dealt by
        /// `cause`.
        pub fn damage_of_kind(
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
        pub fn probe_as(
            world: &mut World,
            source: &str,
            role: ScriptRole,
            actor: StableId,
            id: StableId,
        ) -> Result<ScriptValue, CallError> {
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
            let returned = ScriptValue::probe(world, source, (ctx.clone(), handle))?;
            ctx.apply(world);
            Ok(returned)
        }
    }
}
