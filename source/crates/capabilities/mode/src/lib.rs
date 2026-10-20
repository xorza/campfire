//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use std::sync::Arc;

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, ActionsSystems};
use campfire_combat::{Weigh, Weighers};
use campfire_common::{EnumAll, Seat};
use campfire_declarations::{ByType, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_navigation::{
    ModeNavigationData, Navigation, NavigationBook, OnPath, PathArrivals, WalkerOnPath,
};
use campfire_players::{MatchEntity, Player, Players, PlayersWorld, SeatHolder, StartingSeat};
use campfire_production::{CompletedSites, ProductionSystems, TrainedUnits};
use campfire_progression::LevelUps;
use campfire_script::{
    ApiVersion, CallError, Ctx, Hook, HookSet, MatchNames, ModeEvents, PlannedCapability,
    ScriptRole, ScriptText, Status,
};
use campfire_sim::{Breach, Capability, IdAllocator, SimSystems, SimTick};
use campfire_units::{
    AudienceAll, AudienceNone, AudienceOwner, AudienceTeam, BodyForm, ScriptBook, SpawnAt, Spawner,
    UnitsSystems,
};
use campfire_values::Relations;

use crate::bounty_book::Bounty;
use crate::game_map::GameMap;
use crate::mode_api::ModeApi;
use crate::mode_call::ModeCall;
use crate::mode_hooks::{
    ModeHooks, OnGenerate, OnInput, OnMatchStart, OnPlayerJoin, OnPlayerLeave, OnQuestStage,
    OnSystem, OnTimer, OnUnitDeath,
};
use crate::mode_script::ModeScript;
use crate::out_of_play_marks::OutOfPlayMarks;
use crate::placed_unit::PlacedPath;
use crate::save_asked::SaveAsked;
use crate::victory::Victory;

mod bounty_book;
mod bounty_data;
mod bounty_share;
mod calls;
mod choice_data;
mod choice_id;
mod choice_specs;
mod choices;
mod error;
mod game_map;
mod loadout;
mod loadout_setup;
mod map_data;
mod marker;
mod marker_spec;
mod marker_tag;
mod match_end;
mod match_mode;
mod mode_api;
mod mode_call;
mod mode_core;
mod mode_ctx;
mod mode_data;
mod mode_effect;
mod mode_hooks;
mod mode_input;
mod mode_inputs;
mod mode_load;
mod mode_map;
mod mode_schema;
mod mode_script;
mod mode_setup;
mod mode_spec;
mod mode_state;
mod mode_teams;
mod mode_units;
mod mode_world;
mod offer;
mod option_data;
mod option_picks;
mod option_value;
mod out_of_play_marks;
mod placed_unit;
mod player_control;
mod player_state;
mod player_state_access;
mod player_states;
mod players_data;
mod region_data;
mod relation_data;
mod result_handle;
mod roster;
mod save_asked;
mod saves_data;
mod spawn_group;
mod team_data;
mod team_manifest;
mod timer_id;
mod timers;
mod unanswered_deaths;
mod unanswered_seat_events;
mod victory;
mod victory_data;

pub use crate::bounty_book::BountyBook;
pub use crate::bounty_data::{BountyData, ModeBountyData, Pays};
pub use crate::bounty_share::BountyShare;
pub use crate::calls::Calls;
pub use crate::choice_data::{ChoiceData, Offers};
pub use crate::choice_id::ChoiceId;
pub use crate::choices::Choices;
pub use crate::error::{
    BountyError, ModeDataError, ModeError, ModeRefusal, OptionDataError, OptionValueError,
    PickError,
};
pub use crate::loadout_setup::LoadoutSetup;
pub use crate::map_data::{MapData, MapPatch, MapSeed, MarkerData, PlacedUnitData};
pub use crate::marker::{Marker, MarkerInfo};
pub use crate::marker_spec::MarkerSpec;
pub use crate::marker_tag::MarkerTag;
pub use crate::match_end::{MatchEnd, MatchResult};
pub use crate::match_mode::MatchMode;
pub use crate::mode_core::ModeCore;
pub use crate::mode_data::{InputType, ListEntry, ModeData, ModeParam, ModePatch, ModeSeed};
pub use crate::mode_input::{InputValue, ModeInput};
pub use crate::mode_inputs::ModeInputs;
pub use crate::mode_load::ModeLoad;
pub use crate::mode_map::{MapInput, ModeMap};
pub use crate::mode_setup::ModeSetup;
pub use crate::mode_spec::{MapStart, ModeSpec, SpecLoad};
pub use crate::mode_state::{ModeAudience, ModeState};
pub use crate::mode_teams::ModeTeams;
pub use crate::mode_units::ModeUnits;
pub use crate::mode_world::ModeWorld;
pub use crate::offer::Offer;
pub use crate::option_data::OptionData;
pub use crate::option_picks::OptionPicks;
pub use crate::option_value::OptionValue;
pub use crate::placed_unit::PlacedUnit;
pub use crate::player_control::PlayerControl;
pub use crate::player_state::{PlayerAudience, PlayerState};
pub use crate::players_data::PlayersData;
pub use crate::region_data::RegionData;
pub use crate::relation_data::RelationData;
pub use crate::saves_data::{SaveBy, SavesData};
pub use crate::team_data::TeamData;
pub use crate::team_manifest::TeamManifest;
pub use crate::timer_id::TimerId;
pub use crate::timers::{Timer, TimerStart, Timers};
pub use crate::unanswered_deaths::{DeathCall, UnansweredDeaths};
pub use crate::unanswered_seat_events::UnansweredSeatEvents;
pub use crate::victory::VictoryBook;
pub use crate::victory_data::{Defeat, VictoryData};

/// The scripted part of the mode of a match: the core's rules above the capabilities, which a
/// match that runs scripts installs after them, once its scripts compile; `ModeCore` is the part
/// every match installs.
#[derive(Debug)]
pub struct Mode;

/// The mode's steps.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ModeSystems {
    /// The tick's mode inputs run its script, once the tick started and before any order.
    Inputs,
    /// Once the trains spawned: the tick's deaths end players and the match, then pay their
    /// bounties.
    Deaths,
    /// Then the seats' events, the due timers and the deaths run the mode's hooks, which read how
    /// the match ended.
    Hooks,
    /// Then the levels units reached run theirs.
    LevelUps,
    /// Then the units trains made.
    Trained,
    /// Then the sites that completed.
    Sites,
    /// Then the walkers that reached their paths' ends.
    PathEnds,
    /// The players out of play are counted again.
    OutOfPlay,
}

impl ModuleSystems for ModeSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Inputs => (SetPlace::stage(SimSystems::Inputs).after(UnitsSystems::Begin))
                .before(ActionsSystems::Toggle),
            Self::Deaths => SetPlace::stage(SimSystems::Mode).after(ProductionSystems::Finish),
            Self::Hooks => SetPlace::stage(SimSystems::Mode).after(Self::Deaths),
            Self::LevelUps => SetPlace::stage(SimSystems::Mode).after(Self::Hooks),
            Self::Trained => SetPlace::stage(SimSystems::Mode).after(Self::LevelUps),
            Self::Sites => SetPlace::stage(SimSystems::Mode).after(Self::Trained),
            Self::PathEnds => SetPlace::stage(SimSystems::Mode).after(Self::Sites),
            Self::OutOfPlay => SetPlace::at(UnitsSystems::End),
        }
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
    ];
    type Systems = ModeSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.callback::<Weighers>(
            (),
            Weigh {
                damage: Calls::weigh_damage,
                heal: Calls::weigh_heal,
            },
        );
        d.component::<ModeState<AudienceNone>>();
        d.component::<ModeState<AudienceAll>>();
        d.component::<PlayerState<AudienceNone>>();
        d.component::<PlayerState<AudienceOwner>>();
        d.component::<PlayerState<AudienceTeam>>();
        d.component::<PlayerState<AudienceAll>>();
        d.resource::<Choices>();
        d.resource::<Timers>();
        d.resource::<UnansweredDeaths>();
        d.resource::<UnansweredSeatEvents>();
        d.report::<SaveAsked>();
        d.loaded::<MatchMode>();
        d.check::<Player>(Self::players_spawned);
        d.part::<MatchEntity, Relations>(|at| {
            Some(at.world().resource::<MatchMode>().relations.clone())
        });
        d.part::<MatchEntity, ModeState<AudienceNone>>(|at| {
            Some(ModeState::initial(
                &at.world().resource::<MatchMode>().schema,
            ))
        });
        d.part::<MatchEntity, ModeState<AudienceAll>>(|at| {
            Some(ModeState::initial(
                &at.world().resource::<MatchMode>().schema,
            ))
        });
        d.part::<Player, PlayerState<AudienceNone>>(|at| {
            Some(PlayerState::initial(
                &at.world().resource::<MatchMode>().schema.players,
            ))
        });
        d.part::<Player, PlayerState<AudienceOwner>>(|at| {
            Some(PlayerState::initial(
                &at.world().resource::<MatchMode>().schema.players,
            ))
        });
        d.part::<Player, PlayerState<AudienceTeam>>(|at| {
            Some(PlayerState::initial(
                &at.world().resource::<MatchMode>().schema.players,
            ))
        });
        d.component::<BountyShare>();
        d.part::<Player, BountyShare>(|at| {
            let start = at.world().resource::<BountyBook>().start();
            start.map(BountyShare)
        });
        d.part::<Player, PlayerState<AudienceAll>>(|at| {
            Some(PlayerState::initial(
                &at.world().resource::<MatchMode>().schema.players,
            ))
        });
        d.api(ModeApi::register);
        d.hook::<OnMatchStart>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnInput>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnTimer>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnUnitDeath>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnPlayerJoin>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnPlayerLeave>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnGenerate>(Status::Planned);
        d.hook::<OnSystem>(Status::Planned);
        d.plan_hook::<OnQuestStage>(PlannedCapability::Quests);
    }

    /// Adds the mode's hooks to a match: in Inputs, the players' mode inputs run
    /// `on_input`; in Mode, the tick's joins and leaves run `on_player_join` and
    /// `on_player_leave`, due timers run `on_timer`, or fire with no call in a mode without it, the
    /// tick's deaths run `on_unit_death`; then, of the capabilities the match holds, the levels
    /// reached run `on_level_up`, the units trained `on_unit_trained`, the sites that completed
    /// `on_site_complete`, and the walkers at their paths' ends `on_path_end`, each in that order,
    /// or clear in a mode without the hook. Each hook's system installs only in a mode whose
    /// script defines its hook, a fact of the load; `Mode::load` gives them the mode they run.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        let book = cx.book::<ScriptBook>();
        let book = book.expect("a match that runs its mode holds its scripts' hooks");
        let hooks = cx.mode_scripts().map_or_else(HookSet::default, |listed| {
            book.defines_any(listed, ScriptRole::Mode)
        });
        if cx
            .book::<VictoryBook>()
            .and_then(|book| book.victory())
            .is_some()
        {
            cx.add(ModeSystems::Deaths, Victory::follow);
        }
        if cx
            .book::<BountyBook>()
            .and_then(BountyBook::bounty)
            .is_some()
        {
            cx.add(ModeSystems::Deaths, Bounty::pay.after(Victory::follow));
        }
        let defines = |hook| hooks.contains(hook);
        let answers = |queue_hooks: &[Hook]| queue_hooks.iter().any(|&hook| defines(hook));
        let timers = (ModeHooks::run_timers, ModeHooks::fire_timers);
        if defines(Hook::of::<OnInput>()) {
            cx.add(ModeSystems::Inputs, ModeHooks::mode_inputs);
        }
        let seat_events = ModeHooks::answer::<UnansweredSeatEvents>;
        if answers(UnansweredSeatEvents::HOOKS) {
            cx.add(
                ModeSystems::Hooks,
                seat_events.before(timers.0).before(timers.1),
            );
        }
        if defines(Hook::of::<OnTimer>()) {
            cx.add(ModeSystems::Hooks, timers.0);
        } else {
            cx.add(ModeSystems::Hooks, timers.1);
        }
        let deaths = ModeHooks::answer::<UnansweredDeaths>;
        if answers(UnansweredDeaths::HOOKS) {
            cx.add(ModeSystems::Hooks, deaths.after(timers.0).after(timers.1));
        }
        cx.add(ModeSystems::OutOfPlay, OutOfPlayMarks::refresh);
        if cx.has(Capability::Progression) {
            Self::events::<LevelUps>(cx, ModeSystems::LevelUps, &hooks);
        }
        if cx.has(Capability::Production) {
            Self::events::<TrainedUnits>(cx, ModeSystems::Trained, &hooks);
            Self::events::<CompletedSites>(cx, ModeSystems::Sites, &hooks);
        }
        if cx.has(Capability::Navigation) {
            Self::events::<PathArrivals>(cx, ModeSystems::PathEnds, &hooks);
        }
    }
}

impl Mode {
    /// Adds, in `set`, the answer of a capability's events `Q` in a mode whose script defines one
    /// of their hooks, `hooks` being those it defines, or else their drop.
    fn events<Q: ModeEvents>(
        cx: &mut InstallCx<'_, ModeSystems>,
        set: ModeSystems,
        hooks: &HookSet,
    ) {
        if Q::HOOKS.iter().any(|&hook| hooks.contains(hook)) {
            cx.add(set, ModeHooks::answer::<Q>);
        } else {
            cx.add(set, ModeHooks::drop::<Q>);
        }
    }

    /// Loads the mode of `setup` into a match whose capabilities, mode and books are installed
    /// and whose unit types, abilities and AI are loaded, with what `inputs` takes from the
    /// books: the map's pathing grid, for the kinds of its walkers, its teams, and the units the
    /// mode spawns.
    pub fn load(world: &mut World, setup: ModeSetup<'_>, inputs: ModeInputs) {
        let ModeInputs { units, load } = inputs;
        let ModeLoad {
            map,
            walkers,
            loadout_ranks,
        } = load;
        Navigation::load_map(world, walkers);
        let ModeMap {
            teams,
            relations,
            placed,
            markers,
        } = map;
        let paths = world
            .get_resource::<NavigationBook>()
            .map(|book| &book.paths);
        let path_names = paths.into_iter().flat_map(|paths| paths.names().texts());
        let spawn_tags = &setup
            .data
            .section_or_default::<ModeNavigationData>()
            .spawn_tags;
        let map = GameMap::new(path_names.map(ScriptText::from), &markers, spawn_tags);
        world
            .resource_mut::<MatchNames>()
            .set_table(Arc::clone(map.tags()));
        let script = ModeScript::new(setup.data, setup.picks, map);
        let start = MapStart {
            teams,
            placed,
            relations,
        };
        let load = SpecLoad {
            start,
            units,
            loadout_ranks,
            scripts: world.resource::<ScriptBook>(),
            bodies: world.resource::<ByType<BodyForm>>(),
            slots: world.resource::<ByType<ActionSlots>>(),
            bounty: world.resource::<BountyBook>().bounty().is_some(),
        };
        let book = ModeSpec::new(setup, load);
        let mut names = world.resource_mut::<MatchNames>();
        names.set_teams(Arc::clone(&book.teams));
        names.set_table(Arc::clone(book.choices.names()));
        names.set_table(Arc::clone(&book.timers));
        let players = usize::try_from(book.teams.players()).expect("a player count fits usize");
        world.insert_resource(Choices::empty(&book.choices, players));
        let ctx = world.non_send::<Ctx>().clone();
        let book = Arc::new(book);
        ctx.frame()
            .add_params(ModeCall::new(Arc::clone(&book), script));
        world.insert_resource(MatchMode(Arc::clone(&book)));
    }

    /// Whether each seat of the session has its entity, as the mode's start spawns them, so no
    /// call finds one missing.
    fn players_spawned(world: &World) -> Result<(), Breach> {
        let players = world.resource::<MatchMode>().teams.players();
        let seated = (0..players)
            .map(Seat::new)
            .all(|seat| world.player_entity(seat).is_some());
        Breach::of_match(seated)
    }

    /// Starts the match, before its first tick: the players spawn, each seat held or open as
    /// `holders` gives it, one for each seat in order; the map's placed units spawn, on their
    /// paths and walking them as they name; then `on_match_start` runs. A timer it sets counts
    /// from the start. An error when the call fails: a match its mode cannot start would run
    /// without its rules.
    pub fn start(world: &mut World, holders: &[SeatHolder]) -> Result<(), CallError> {
        let book = world.resource::<MatchMode>().clone();
        let teams = &book.teams;
        assert_eq!(
            holders.len(),
            usize::try_from(teams.players()).expect("seats fit usize"),
            "a start gives each seat of the session its holder"
        );
        let seats = (0..teams.players()).map(Seat::new).zip(holders);
        Players::spawn_match(
            world,
            seats.map(|(seat, &holder)| StartingSeat {
                team: teams.of(seat).expect("every player has a team"),
                holder,
            }),
        );
        for placed in &book.placed {
            let at = SpawnAt {
                id: world.resource_mut::<IdAllocator>().allocate(),
                unit_type: placed.unit_type,
                team: placed.team,
                position: placed.position,
                owner: None,
                angle: placed.angle,
            };
            match placed.path {
                None => Spawner::spawn(world, at, ()),
                Some(PlacedPath { path, from: None }) => {
                    Spawner::spawn(world, at, OnPath::new(path))
                }
                Some(PlacedPath {
                    path,
                    from: Some(from),
                }) => Spawner::spawn(world, at, WalkerOnPath::new(path, from, at.id)),
            };
        }
        if !book.schema.hooks.contains(Hook::of::<OnMatchStart>()) {
            return Ok(());
        }
        let now = world.resource::<SimTick>().start();
        Calls::scope(world, now, |call| {
            let ctx = call.ctx.clone();
            call.run::<OnMatchStart>(|| (ctx.clone(),))
        })?;
        // A save the start asked for is at the boundary before the first tick, its own time.
        if world.contains_resource::<SaveAsked>() {
            world.insert_resource(SaveAsked::new(now));
        }
        Ok(())
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::bundle::{Bundle, NoBundleEffect};
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_actions::Actions;
    use campfire_combat::{Damage, DamageCause, DamagePass, PassEntry};
    use campfire_common::Seat;
    use campfire_math::{Num, Vec3};
    use campfire_navigation::{PathEnd, PathWalker};
    use campfire_players::SeatHolder;
    use campfire_script::FailureKind;
    use campfire_script::{
        CallError, CallStart, Ctx, MatchNames, ScriptFailures, ScriptRole, ScriptValue, StateValue,
    };
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{
        EntityIndex, IdAllocator, Position, SeatEvent, SeatEventKind, SimTick, SimWorld, StableId,
        TickInputs,
    };
    use campfire_stats::{Application, Modifiers, NewInstance, StatBook, StatOp, StatShare, Stats};
    use campfire_units::{Life, Owner, SpawnAt, Spawner, Units};
    use campfire_values::{NameId, PackageIndex, Rank, Stat, TagSet, Team, UnitType};

    use crate::Mode;
    use crate::match_mode::MatchMode;
    use crate::mode_call::ModeCall;
    use crate::mode_ctx::ModeCtx;
    use crate::mode_input::{InputValue, ModeInput};
    use crate::mode_world::ModeWorld;

    /// A marker a reader finds: of the tag `tag`, whose param `param` is the text `value`.
    #[derive(Debug, Clone, Copy)]
    pub struct MarkerOf<'a> {
        pub tag: &'a str,
        pub param: &'a str,
        pub value: &'a str,
    }

    impl Mode {
        /// The point of the marker `of` names, as the match's map holds it, which its mode's
        /// script reads; none for no such marker, or one of no point.
        pub fn marker_point(world: &World, of: MarkerOf<'_>) -> Option<Vec3> {
            let ctx = world.non_send::<Ctx>();
            let frame = ctx.frame();
            frame.extension::<ModeCall>()?.map().point_of(of)
        }

        /// Spawns a unit of the mode's type `name` at `position` on `team`, of the next stable id,
        /// with `parts`, as the mode's own spawns do.
        pub fn spawn_typed(
            world: &mut World,
            name: &str,
            position: Vec3,
            team: Team,
            parts: impl Bundle<Effect: NoBundleEffect>,
        ) -> StableId {
            let unit_type =
                Units::type_named(world, name).unwrap_or_else(|| panic!("no unit type {name}"));
            Self::spawn_at(world, unit_type, position, Num::ZERO, team, parts)
        }

        /// Spawns a unit of the mode's type `unit_type` at `position`, turned `angle` degrees, on
        /// `team`, of the next stable id, with `parts`, through the match's spawner, as the mode's
        /// own spawns do.
        pub(super) fn spawn_at(
            world: &mut World,
            unit_type: UnitType,
            position: Vec3,
            angle: Num,
            team: Team,
            parts: impl Bundle<Effect: NoBundleEffect>,
        ) -> StableId {
            let id = world.resource_mut::<IdAllocator>().allocate();
            let at = SpawnAt {
                id,
                unit_type,
                team,
                owner: None,
                position,
                angle,
            };
            Spawner::place_any(world, at, parts);
            id
        }
    }

    impl Mode {
        /// Spawns a unit of the mode's type `unit_type` at `position` on `team`, of the next stable
        /// id, with `parts`, as the mode's own spawns do, once the match loaded its types: the
        /// first spawn seals their tags, with no effect for a tag, as a mode's load does once
        /// its packages load. On a client's match too, as the server would send the unit.
        pub fn spawn(
            world: &mut World,
            unit_type: UnitType,
            position: impl Into<Vec3>,
            team: Team,
            parts: impl Bundle<Effect: NoBundleEffect>,
        ) -> StableId {
            Self::spawn_turned(world, unit_type, position, Num::ZERO, team, parts)
        }

        /// Spawns a unit as `spawn` does, turned `angle` degrees.
        pub fn spawn_turned(
            world: &mut World,
            unit_type: UnitType,
            position: impl Into<Vec3>,
            angle: Num,
            team: Team,
            parts: impl Bundle<Effect: NoBundleEffect>,
        ) -> StableId {
            Units::seal_once(world);
            Self::spawn_at(world, unit_type, position.into(), angle, team, parts)
        }

        /// Starts the match as `start` does, every seat held.
        pub fn start_held(world: &mut World) -> Result<(), CallError> {
            Self::start(world, &vec![SeatHolder::Held; Self::seats(world)])
        }

        /// How many seats the session of the installed mode has.
        pub fn seats(world: &World) -> usize {
            let players = world.resource::<MatchMode>().teams.players();
            usize::try_from(players).expect("seats fit usize")
        }

        /// Spawns a unit of the mode's type `name`, as `spawn` does.
        pub fn spawn_named(
            world: &mut World,
            name: &str,
            position: impl Into<Vec3>,
            team: Team,
            parts: impl Bundle<Effect: NoBundleEffect>,
        ) -> StableId {
            let unit_type =
                Units::type_named(world, name).unwrap_or_else(|| panic!("no unit type {name}"));
            Self::spawn(world, unit_type, position, team, parts)
        }

        /// Player `seat` picks `hero` in a tick: the hero it then owns.
        pub fn pick(world: &mut World, seat: u32, hero: &str) -> Entity {
            let input = ModeInput {
                name: "hero",
                value: InputValue::String(hero),
            };
            Self::tick_inputs(world, &[(seat, input)]);
            let player = Seat::new(seat);
            let mut owned = world.query::<(Entity, &Owner)>();
            let mut heroes = owned
                .iter(world)
                .filter(|(_, owner)| owner.seat() == player);
            let (hero, _) = heroes.next().expect("the player picked a hero");
            assert!(heroes.next().is_none(), "the player owns one hero");
            hero
        }

        /// Runs a tick with `inputs`, each a player's seat and a mode input.
        pub fn tick_inputs(world: &mut World, inputs: &[(u32, ModeInput<'_>)]) {
            let (mut body, mut payload) = (Vec::new(), Vec::new());
            for (seat, input) in inputs {
                payload.clear();
                input.write_payload(&mut body, &mut payload);
                world.push_input(Seat::new(*seat), &payload);
            }
            world.step();
        }

        /// Runs a tick in which the session logged `events`, each a player's join or leave.
        pub fn tick_seats(world: &mut World, events: &[(u32, SeatEventKind)]) {
            for &(seat, kind) in events {
                let seat = Seat::new(seat);
                let mut inputs = world.resource_mut::<TickInputs>();
                inputs.push_seat_event(SeatEvent { seat, kind });
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
            ["phase", "seen", "count", "inputs"].map(|name| Self::field(world, name))
        }

        /// The state field `name`: the state holds the fields in the order of their names.
        pub fn field(world: &World, name: &str) -> StateValue {
            let ctx = world.non_send::<Ctx>();
            let at = ctx
                .mode_spec()
                .unwrap()
                .schema
                .state
                .field(name)
                .unwrap()
                .index();
            world.mode_state()[at].clone()
        }

        /// Each unit: its id, where it stands, its team, and the end of a path it walks from.
        pub fn placed_units(world: &World) -> Vec<(u64, Vec3, usize, Option<PathEnd>)> {
            world
                .resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| world.entity(entity).contains::<UnitType>())
                .map(|(id, entity)| {
                    let unit = world.entity(entity);
                    (
                        id.get(),
                        unit.get::<Position>().unwrap().get(),
                        unit.get::<Team>().unwrap().index(),
                        unit.get::<PathWalker>().map(|walker| walker.walks_from()),
                    )
                })
                .collect()
        }

        /// The kind of each of the tick's failed calls.
        pub fn failures(world: &World) -> Vec<FailureKind> {
            let failures = world.resource::<ScriptFailures>().calls();
            failures.into_iter().map(|failure| failure.kind).collect()
        }

        /// A grunt of the mode's on `team`, of 1000 health in place of its type's, whose modifier
        /// adds `stats` by name; with no stats, it carries no modifier, and the match's books stay
        /// as they are, so a grunt spawns at any tick.
        pub fn stat_grunt(world: &mut World, team: Team, stats: &[(&str, Num)]) -> StableId {
            let grunt = Units::type_named(world, "grunt").unwrap();
            let life = Life::full(Num::int(1000));
            if stats.is_empty() {
                return Self::spawn(world, grunt, Vec3::ZERO, team, life);
            }
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
            let id = Stats::push_modifier_changes(world, &changes, TagSet::NONE);
            let shares = stats
                .iter()
                .map(|&(_, value)| StatShare { value, live: None });
            let instance = NewInstance {
                stats: shares.collect(),
                ..NewInstance::bare(id, None)
            };
            let modifiers = Modifiers::bundle([Application::refresh(instance)]);
            Self::spawn(world, grunt, Vec3::ZERO, team, (life, modifiers))
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
                .resource::<MatchNames>()
                .named(kind)
                .expect("the mode names the damage kind");
            DamagePass::queue(
                world,
                PassEntry::Damage(Damage {
                    source,
                    target,
                    amount: Num::int(amount),
                    kind,
                    cause,
                    action: None,
                    depth: 0,
                    hit: None,
                }),
            );
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
                ScriptRole::Mode => {
                    let place = world.resource::<MatchMode>().schema.mode;
                    let start = CallStart::listed(place, now);
                    ctx.frame().begin_alone(world, start).unwrap();
                }
                ScriptRole::Ai => ctx
                    .frame()
                    .begin_alone(world, CallStart::think(actor, now))
                    .unwrap(),
                ScriptRole::Action => {
                    let strike = Actions::named(world, PackageIndex::MODE, "strike").unwrap();
                    let start =
                        CallStart::cast(strike, Rank::FIRST, actor, PackageIndex::MODE, now);
                    ctx.frame().begin_alone(world, start).unwrap();
                }
                ScriptRole::Modifier => {
                    let blessing = Stats::modifier(world, PackageIndex::MODE, "blessing").unwrap();
                    let start = CallStart {
                        acting: Some(actor),
                        ..CallStart::hook(blessing, PackageIndex::MODE, 1, now)
                    };
                    ctx.frame().begin_alone(world, start).unwrap();
                }
            }
            let handle = ctx.view().unit(id).unwrap();
            let returned = ScriptValue::probe(world, source, (ctx.clone(), handle))?;
            ctx.apply(world);
            Ok(returned)
        }
    }
}
