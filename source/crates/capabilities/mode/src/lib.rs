//! The mode: its script, its state, its players' choices, and the units it spawns. It runs the
//! mode's hooks on the capabilities below it.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use std::sync::Arc;

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::{ActionSlots, ActionsSystems};
use campfire_combat::{Weigh, Weighers};
use campfire_common::{EnumAll, PlayerSlot};
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_names::MatchNames;
use campfire_navigation::{ModeNavigationData, NavigationBook, OnPath, WalkerOnPath};
use campfire_players::{
    MatchEntity, Player, PlayerSlotHolder, Players, PlayersWorld, StartingPlayerSlot,
};
use campfire_production::ProductionSystems;
use campfire_script::{
    ApiVersion, CallError, Ctx, Hook, HookSet, ModeEvents, PlannedCapability, ScriptRole,
    ScriptText, Status,
};
use campfire_sim::{Breach, Capability, IdAllocator, SimSystems, SimTick};
use campfire_units::{
    AudienceAll, AudienceNone, AudienceOwner, AudienceTeam, BodyForm, Owner, ScriptBook, SpawnAt,
    Spawner, UnitEntity, UnitsSystems,
};
use campfire_values::{Relations, UnitType};

use crate::bounty_book::Bounty;
use crate::game_map::GameMap;
use crate::mode_api::ModeApi;
use crate::mode_call::ModeCall;
use crate::mode_hooks::{
    ModeHooks, OnGenerate, OnInput, OnMatchStart, OnPlayerJoin, OnPlayerLeave, OnQuestStage,
    OnSystem, OnTimer,
};
use crate::mode_script::ModeScript;
use crate::out_of_play_marks::OutOfPlayMarks;
use crate::placed_unit::PlacedPath;
use crate::save_asked::SaveAsked;
use crate::type_income::TypeIncome;
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
mod host_picks;
mod income_clock;
mod income_data;
mod input_book;
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
mod mode_queue;
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
mod player_slot_pick;
mod player_slot_picks;
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
mod type_income;
mod unanswered_player_slot_events;
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
    BountyError, HostPickError, IncomeError, ModeDataError, ModeError, ModeRefusal,
    OptionDataError, OptionValueError, PickError, PlayerSlotPickError, PlayerSlotPickTextError,
};
pub use crate::host_picks::HostPicks;
pub use crate::income_clock::{CaptureBonus, IncomeClock};
pub use crate::income_data::{IncomeData, IncomePays};
pub use crate::input_book::InputBook;
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
pub use crate::mode_queue::{ModeQueue, ModeQueues};
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
pub use crate::player_slot_pick::PlayerSlotPick;
pub use crate::player_slot_picks::PlayerSlotPicks;
pub use crate::player_state::{PlayerAudience, PlayerState};
pub use crate::players_data::PlayersData;
pub use crate::region_data::RegionData;
pub use crate::relation_data::RelationData;
pub use crate::saves_data::{SaveBy, SavesData};
pub use crate::team_data::TeamData;
pub use crate::team_manifest::TeamManifest;
pub use crate::timer_id::TimerId;
pub use crate::timers::{Timer, TimerStart, Timers};
pub use crate::unanswered_player_slot_events::UnansweredPlayerSlotEvents;
pub use crate::victory::{VictoryBook, VictoryHolders};
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
    /// Then the units of an income pay their owners.
    Income,
    /// Then the player slots' events and the due timers run the mode's hooks, which read how the
    /// match ended.
    Hooks,
    /// Then the queues of events the modules declare run theirs, in the order the modules
    /// declare them.
    Events,
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
            Self::Income => SetPlace::stage(SimSystems::Mode).after(Self::Deaths),
            Self::Hooks => SetPlace::stage(SimSystems::Mode).after(Self::Income),
            Self::Events => SetPlace::stage(SimSystems::Mode).after(Self::Hooks),
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
        d.resource::<VictoryHolders>();
        d.resource::<UnansweredPlayerSlotEvents>();
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
        d.component::<IncomeClock>();
        d.part::<UnitEntity, IncomeClock>(|at| {
            let income = at.of_type::<TypeIncome>()?;
            let now = at.world().resource::<SimTick>().start();
            let owner = at
                .world()
                .get::<Owner>(at.entity())
                .map(|owner| owner.player_slot());
            Some(IncomeClock::spawned(now, owner, income))
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
        d.hook::<OnPlayerJoin>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnPlayerLeave>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnGenerate>(Status::Planned);
        d.hook::<OnSystem>(Status::Planned);
        d.plan_hook::<OnQuestStage>(PlannedCapability::Quests);
    }

    /// Adds the mode's hooks to a match: in Inputs, the players' mode inputs run
    /// `on_input`; in Mode, the tick's joins and leaves run `on_player_join` and
    /// `on_player_leave`, and due timers run `on_timer`, or fire with no call in a mode without
    /// it; then each queue of events a module declares runs its hooks, in the order the modules
    /// declare them, or clears in a mode without them. Each hook's system installs only in a mode
    /// whose script defines its hook, a fact of the load; `Mode::load` gives them the mode they
    /// run.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        let book = cx.book::<ScriptBook>();
        let book = book.expect("a match that runs its mode holds its scripts' hooks");
        let hooks = cx.mode_scripts().map_or_else(HookSet::default, |listed| {
            book.defines_any(listed, ScriptRole::Mode)
        });
        // The load refuses a victory and a bounty in a match without combat, whose deaths they
        // read.
        let deaths = cx.has(Capability::Combat);
        if deaths
            && cx
                .book::<VictoryBook>()
                .and_then(|book| book.victory())
                .is_some()
        {
            cx.add(ModeSystems::Deaths, Victory::follow);
        }
        if deaths
            && cx
                .book::<BountyBook>()
                .and_then(BountyBook::bounty)
                .is_some()
        {
            cx.add(ModeSystems::Deaths, Bounty::pay.after(Victory::follow));
        }
        let incomes = cx.book::<ById<UnitType, TypeIncome>>();
        if incomes.is_some_and(|incomes| !incomes.is_empty()) {
            cx.add(ModeSystems::Income, IncomeClock::pay);
        }
        let defines = |hook| hooks.contains(hook);
        let timers = (ModeHooks::run_timers, ModeHooks::fire_timers);
        if defines(Hook::of::<OnInput>()) {
            cx.add(ModeSystems::Inputs, ModeHooks::mode_inputs);
        }
        let player_slot_events = ModeHooks::answer::<UnansweredPlayerSlotEvents>;
        if UnansweredPlayerSlotEvents::HOOKS
            .iter()
            .any(|&hook| defines(hook))
        {
            cx.add(
                ModeSystems::Hooks,
                player_slot_events.before(timers.0).before(timers.1),
            );
        }
        if defines(Hook::of::<OnTimer>()) {
            cx.add(ModeSystems::Hooks, timers.0);
        } else {
            cx.add(ModeSystems::Hooks, timers.1);
        }
        cx.add(ModeSystems::OutOfPlay, OutOfPlayMarks::refresh);
        let queues: Vec<ModeQueue> = cx
            .declared::<ModeQueues>()
            .map_or_else(Vec::new, |queues| queues.iter().collect());
        let mut before = None;
        for queue in queues {
            queue.add(cx, &hooks, before);
            before = Some(queue);
        }
    }
}

impl Mode {
    /// Loads the mode of `setup` into a match whose capabilities, mode and books are installed and
    /// whose unit types, abilities and AI and the map's pathing grid are loaded, with what `inputs`
    /// takes from the books: its teams, and the units the mode spawns. Each player
    /// slot's choices start as its host picked them, so the start reads them (08-sessions.md, The
    /// terms hold each player slot's picks).
    pub fn load(world: &mut World, setup: ModeSetup<'_>, inputs: ModeInputs) {
        let ModeInputs { units, load } = inputs;
        let ModeLoad { map, loadout_ranks } = load;
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
        let script = ModeScript::new(setup.data, &setup.picks.options, map);
        let (data, player_slot_picks) = (setup.data, &setup.picks.player_slots);
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
            bodies: world.resource::<ById<UnitType, BodyForm>>(),
            slots: world.resource::<ById<UnitType, ActionSlots>>(),
            bounty: world.resource::<BountyBook>().bounty().is_some(),
        };
        let book = ModeSpec::new(setup, load);
        let mut names = world.resource_mut::<MatchNames>();
        names.set_teams(Arc::clone(&book.teams));
        names.set_table(Arc::clone(book.choices.names()));
        names.set_table(Arc::clone(&book.timers));
        let players = usize::try_from(book.teams.players()).expect("a player count fits usize");
        let mut choices = Choices::empty(&book.choices, players);
        for picked in player_slot_picks.picked(&data.choices) {
            let choice = book.choices.listed().nth(picked.listed);
            let choice =
                choice.expect("a player slot pick is of one of the mode's choices of values");
            let offer = Offer::new(usize::from(picked.place));
            choices.choose(&book.choices, picked.player_slot, choice, &[offer]);
        }
        world.insert_resource(choices);
        let ctx = world.non_send::<Ctx>().clone();
        let book = Arc::new(book);
        ctx.frame()
            .add_params(ModeCall::new(Arc::clone(&book), script));
        world.insert_resource(MatchMode(Arc::clone(&book)));
    }

    /// Whether each player slot of the session has its entity, as the mode's start spawns them, so
    /// no call finds one missing.
    fn players_spawned(world: &World) -> Result<(), Breach> {
        let players = world.resource::<MatchMode>().teams.players();
        let held = (0..players)
            .map(PlayerSlot::new)
            .all(|player_slot| world.player_entity(player_slot).is_some());
        Breach::of_match(held)
    }

    /// Starts the match, before its first tick: the players spawn, each player slot held or open as
    /// `holders` gives it, one for each player slot in order; the map's placed units spawn, on
    /// their paths and walking them as they name; then `on_match_start` runs, and the victory
    /// counts the players that hold a unit of its tag. A timer it sets counts from the start. An
    /// error when the call fails: a match its mode cannot start would run without its rules.
    pub fn start(world: &mut World, holders: &[PlayerSlotHolder]) -> Result<(), CallError> {
        let book = world.resource::<MatchMode>().clone();
        let teams = &book.teams;
        assert_eq!(
            holders.len(),
            usize::try_from(teams.players()).expect("player slots fit usize"),
            "a start gives each player slot of the session its holder"
        );
        let player_slots = (0..teams.players()).map(PlayerSlot::new).zip(holders);
        Players::spawn_match(
            world,
            player_slots.map(|(player_slot, &holder)| StartingPlayerSlot {
                team: teams.of(player_slot).expect("every player has a team"),
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
                variation: placed.variation,
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
        if book.schema.hooks.contains(Hook::of::<OnMatchStart>()) {
            let now = world.resource::<SimTick>().start();
            Calls::scope(world, now, |call| {
                let ctx = call.ctx.clone();
                call.run::<OnMatchStart>(|| (ctx.clone(),))
            })?;
            // A save the start asked for is at the boundary before the first tick, its own time.
            if world.contains_resource::<SaveAsked>() {
                world.insert_resource(SaveAsked::new(now));
            }
        }
        Victory::start(world);
        Ok(())
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::mode_world::internals::ModeWorldInternals;

    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use campfire_actions::Actions;
    use campfire_combat::{Damage, DamageCause, DamagePass, PassEntry};
    use campfire_common::PlayerSlot;
    use campfire_math::{Num, Vec3};
    use campfire_names::MatchNames;
    use campfire_navigation::{PathEnd, PathWalker};
    use campfire_players::PlayerSlotHolder;
    use campfire_script::FailureKind;
    use campfire_script::{
        ActionCast, CallError, CallStart, Ctx, ScriptFailures, ScriptRole, ScriptValue, StateValue,
    };
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{
        EntityIndex, PlayerSlotEvent, PlayerSlotEventKind, Position, SimTick, SimWorld, StableId,
        TickInputs, UnreadCommand,
    };
    use campfire_stats::Stats;
    use campfire_units::{Owner, UnitsWorld};
    use campfire_values::{NameId, PackageIndex, PlayerView, Rank, Team, UnitType};

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

        /// Starts the match as `start` does, every player slot held.
        pub fn start_held(world: &mut World) -> Result<(), CallError> {
            Self::start(
                world,
                &vec![PlayerSlotHolder::Held; Self::player_slots(world)],
            )
        }

        /// How many player slots the session of the installed mode has.
        pub fn player_slots(world: &World) -> usize {
            let players = world.resource::<MatchMode>().teams.players();
            usize::try_from(players).expect("player slots fit usize")
        }

        /// Player `player_slot` picks `hero` in a tick: the hero it then owns.
        pub fn pick(world: &mut World, player_slot: u32, hero: &str) -> Entity {
            let input = ModeInput {
                name: "hero",
                value: InputValue::String(hero),
            };
            Self::tick_inputs(world, &[(player_slot, input)]);
            let player = PlayerSlot::new(player_slot);
            let mut owned = world.query::<(Entity, &Owner)>();
            let mut heroes = owned
                .iter(world)
                .filter(|(_, owner)| owner.player_slot() == player);
            let (hero, _) = heroes.next().expect("the player picked a hero");
            assert!(heroes.next().is_none(), "the player owns one hero");
            hero
        }

        /// Runs a tick with `inputs`, each a player's player slot and a mode input.
        pub fn tick_inputs(world: &mut World, inputs: &[(u32, ModeInput<'_>)]) {
            let payloads = Self::input_payloads(inputs);
            world.step_with(&Self::sent(&payloads));
        }

        /// Runs a tick as `tick_inputs` does, and takes the commands nothing read, which a test
        /// that sends them asserts on.
        pub fn tick_inputs_unread(
            world: &mut World,
            inputs: &[(u32, ModeInput<'_>)],
        ) -> Vec<UnreadCommand> {
            let payloads = Self::input_payloads(inputs);
            world.step_with_unread(&Self::sent(&payloads))
        }

        /// The payload of each of `inputs`, by its player slot.
        fn input_payloads(inputs: &[(u32, ModeInput<'_>)]) -> Vec<(PlayerSlot, Vec<u8>)> {
            let mut body = Vec::new();
            let mut payload = |input: &ModeInput<'_>| {
                let mut payload = Vec::new();
                input.write_payload(&mut body, &mut payload);
                payload
            };
            inputs
                .iter()
                .map(|(player_slot, input)| (PlayerSlot::new(*player_slot), payload(input)))
                .collect()
        }

        /// `payloads` as a tick takes them.
        fn sent(payloads: &[(PlayerSlot, Vec<u8>)]) -> Vec<(PlayerSlot, &[u8])> {
            payloads
                .iter()
                .map(|(player_slot, payload)| (*player_slot, payload.as_slice()))
                .collect()
        }

        /// Runs a tick in which the session logged `events`, each a player's join or leave.
        pub fn tick_player_slots(world: &mut World, events: &[(u32, PlayerSlotEventKind)]) {
            for &(player_slot, kind) in events {
                let player_slot = PlayerSlot::new(player_slot);
                let mut inputs = world.resource_mut::<TickInputs>();
                inputs.push_player_slot_event(PlayerSlotEvent { player_slot, kind });
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

        /// Takes the kind of each of the tick's failed calls, which a test that makes them expects.
        pub fn failures(world: &mut World) -> Vec<FailureKind> {
            let failures = world.resource_mut::<ScriptFailures>().take_calls();
            failures.into_iter().map(|failure| failure.kind).collect()
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
                    ctx.begin_alone(world, start).unwrap();
                }
                ScriptRole::Ai => ctx
                    .begin_alone(world, CallStart::think(actor, now))
                    .unwrap(),
                ScriptRole::Bot => {
                    let player_slot = PlayerSlot::new(0);
                    let team = world
                        .team_of(player_slot)
                        .expect("player slot 0 plays the mode");
                    let player = PlayerView {
                        player_slot,
                        team,
                        out: false,
                    };
                    ctx.begin_alone(world, CallStart::bot(player, now)).unwrap();
                }
                ScriptRole::Action => {
                    let strike = Actions::named(world, PackageIndex::MODE, "strike").unwrap();
                    let cast = ActionCast {
                        action: strike,
                        rank: Rank::FIRST,
                        caster: actor,
                        allegiance: world.allegiance_of(actor).unwrap(),
                        package: PackageIndex::MODE,
                    };
                    let start = CallStart::cast(cast, now);
                    ctx.begin_alone(world, start).unwrap();
                }
                ScriptRole::Modifier => {
                    let blessing = Stats::modifier(world, PackageIndex::MODE, "blessing").unwrap();
                    let start = CallStart {
                        acting: Some(actor),
                        ..CallStart::hook(blessing, PackageIndex::MODE, 1, now)
                    };
                    ctx.begin_alone(world, start).unwrap();
                }
            }
            let handle = ctx.view().unit(id).unwrap();
            let returned = ScriptValue::probe(world, source, (ctx.clone(), handle))?;
            ctx.apply(world);
            Ok(returned)
        }
    }
}
