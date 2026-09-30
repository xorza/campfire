use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::slice;

use bevy_ecs::entity::Entity;
use campfire_content::PackagePath;
use campfire_math::{Num, Vec3};
use campfire_script::ScriptId;
use campfire_sim::{Capability, EntityIndex, SimUpdate, Tick, TickInput};

use super::*;
use crate::abilities::Abilities;
use crate::abilities::ability_book::AbilityId;
use crate::abilities::ability_data::{AbilityData, Targeting};
use crate::abilities::ability_slots::AbilitySlots;
use crate::capability_set::internals::TestMatch;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::combat::strikes::{Strike, Strikes};
use crate::mode::hero_index::HeroIndex;
use crate::mode::map_data::{LaneData, NeutralSpawnData, StructureData};
use crate::mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
use crate::mode::mode_setup::{HeroSetup, SpellSetup, UnitTypeSetup};
use crate::mode::spell_index::SpellIndex;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::lane_walker::{LaneWalker, PathDirection};
use crate::navigation::move_step::MoveStep;
use crate::scripts::error::ApiError;
use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::state_decl::{StateDecl, StateDefault, StateType, SyncTo};
use crate::scripts::state_value::StateValue;
use crate::units::Units;
use crate::units::lane::Lane;
use crate::units::owner::Owner;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::grid::Grid;
use crate::values::scalar::Scalar;
use crate::vision::vision_grid::VisionGrid;

/// 10 ticks a second: 100 ms is a tick.
const RATE: TickRate = TickRate::new(NonZeroU32::new(10).unwrap());
const LIMITS: ScriptLimits = ScriptLimits {
    per_call: 10_000,
    player: 100_000,
    think: 100_000,
    mode: 100_000,
};

/// A mode that records what its hooks see in its state, and acts on its players' inputs.
const SCRIPT: &str = r#"
fn on_match_start(ctx) {
    ctx.state.phase = "start";
    ctx.timer("once", 250, false, 7);
    ctx.timer("every", 100, true, ());
    for point in ctx.map.neutral_spawns {
        ctx.spawn_unit(point.unit_type, "neutral", point.pos);
    }
    ctx.spawn_wave("a", "mid", ctx.p.wave);
    ctx.spawn_wave("b", "mid", ["grunt"]);
}

fn on_timer(ctx, name, data) {
    if name == "once" {
        ctx.state.seen = data;
    } else {
        ctx.state.count += 1;
    }
}

fn on_mode_input(ctx, player, name, value) {
    ctx.state.inputs += 1;
    if name == "hero" {
        ctx.choose_hero(player, value);
        ctx.spawn_heroes();
    } else if name == "spells" {
        ctx.choose_spells(player, value);
    } else if name == "rich" {
        ctx.add_resource(player, value, 9223372036854775807);
    } else if name == "gold" {
        ctx.add_resource(player, value, ctx.p.gold);
        ctx.add_resource(player, value, ctx.p.gold);
    } else if name == "fail" {
        ctx.spawn_unit("grunt", "a", ctx.map.neutral_spawns[0].pos);
        throw value;
    } else if name == "phase" {
        ctx.state.phase = 5;
    } else if name == "probe" {
        let grunts = ctx.units_tagged("grunt");
        ctx.state.enemy = ctx.enemy_team("a");
        ctx.state.grunts = grunts.len();
        ctx.state.heroes = ctx.heroes("b").len();
        ctx.state.teams = ctx.teams.len();
        ctx.state.players = ctx.players;
        ctx.state.lane = grunts[1].lane;
        ctx.state.team = grunts[1].team;
        ctx.state.neutral = grunts[0].team;
        ctx.state.owner = ctx.heroes()[0].owner;
        ctx.state.tower_lane = ctx.units_tagged("tower")[0].lane;
        ctx.state.kind = grunts[1].unit_type;
    }
}
"#;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn point(x: i64, z: i64) -> GroundPoint {
    GroundPoint([Scalar::Int(x), Scalar::Int(z)])
}

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
}

fn field(kind: StateType, default: Option<StateDefault>) -> StateDecl {
    StateDecl::new(kind, default, Some(SyncTo::All)).unwrap()
}

/// The unit kit of a grunt: 10 health, an attack, and a step of 1 m.
fn grunt() -> UnitKit {
    UnitKit {
        combatant: Some(Combatant {
            health: Health::new(num(10)).unwrap(),
            attack: Some(
                AttackStats::new(num(1), Ticks::new(0), Ticks::new(1), Num::ZERO).unwrap(),
            ),
            on_death: OnDeath::Stay,
        }),
        step: Some(MoveStep::new(Num::ONE).unwrap()),
        sight: None,
    }
}

/// A grid of 1 m cells from (−10, −5) to (10, 6); one lane, `mid`, along x; team a's spawn at
/// z = −5 and b's at 5; a's tower 8 m down the lane; and a neutral grunt in the middle.
fn map() -> MapData {
    MapData {
        grid: Grid::new(num(1), [num(-10), num(-5)], [num(10), num(6)]),
        lanes: vec![LaneData {
            name: "mid".to_owned(),
            points: vec![point(-10, 0), point(0, 0), point(10, 0)],
        }],
        spawns: [("a", point(0, -5)), ("b", point(0, 5))]
            .map(|(team, point)| (team.to_owned(), point))
            .into(),
        structures: vec![StructureData {
            unit_type: "tower".to_owned(),
            team: "a".to_owned(),
            lane: Some("mid".to_owned()),
            pos: point(-8, 0),
        }],
        neutral_spawns: vec![NeutralSpawnData {
            unit_type: "grunt".to_owned(),
            pos: point(0, 0),
        }],
    }
}

/// The test mode's own files: its data, its map and its teams.
#[derive(Debug)]
struct ModeFiles {
    data: ModeData,
    map: MapData,
    teams: Vec<TeamManifest>,
}

fn mode_files() -> ModeFiles {
    let text = |text: &str| ListEntry::Text(text.to_owned());
    ModeFiles {
        data: ModeData {
            script: PackagePath::parse("scripts/mode.rhai").unwrap(),
            assist_window_ms: None,
            inputs: [
                ("hero", InputType::String),
                ("spells", InputType::StringList),
                ("rich", InputType::String),
                ("gold", InputType::String),
                ("fail", InputType::String),
                ("phase", InputType::String),
                ("probe", InputType::String),
            ]
            .map(|(name, kind)| (name.to_owned(), kind))
            .into(),
            state_version: None,
            state: [
                (
                    "phase",
                    field(StateType::String, Some(StateDefault::Text("pick".into()))),
                ),
                ("seen", field(StateType::Int, None)),
                ("count", field(StateType::Int, None)),
                ("inputs", field(StateType::Int, None)),
                ("enemy", field(StateType::String, None)),
                ("grunts", field(StateType::Int, None)),
                ("heroes", field(StateType::Int, None)),
                ("teams", field(StateType::Int, None)),
                ("players", field(StateType::Int, None)),
                ("lane", field(StateType::String, None)),
                ("team", field(StateType::String, None)),
                ("neutral", field(StateType::String, None)),
                ("owner", field(StateType::Int, None)),
                ("tower_lane", field(StateType::String, None)),
                ("kind", field(StateType::String, None)),
            ]
            .map(|(name, decl)| (name.to_owned(), decl))
            .into(),
            params: [
                ("wave", ModeParam::List(vec![text("grunt"), text("grunt")])),
                ("gold", ModeParam::Value(Scalar::Int(8))),
            ]
            .map(|(name, param)| (name.to_owned(), param))
            .into(),
            modifiers: BTreeMap::new(),
        },
        map: map(),
        teams: vec![
            TeamManifest {
                name: "a".to_owned(),
                slots: 2,
            },
            TeamManifest {
                name: "b".to_owned(),
                slots: 1,
            },
        ],
    }
}

/// The test mode's setup from `files`, of `script`: its grunt, tower and two heroes' unit types,
/// and its one spell.
fn setup(
    files: &ModeFiles,
    script: ScriptId,
    types: [UnitType; 4],
    spell: SpellSetup,
) -> ModeSetup<'_> {
    let [grunt_type, tower_type, x, y] = types;
    let hero = |id: &str, unit_type| HeroSetup {
        id: id.to_owned(),
        unit_type,
        abilities: Vec::new(),
        resource: None,
    };
    ModeSetup {
        script,
        data: &files.data,
        map: &files.map,
        teams: &files.teams,
        players: 3,
        unit_types: vec![
            UnitTypeSetup {
                unit_type: grunt_type,
                kit: grunt(),
            },
            UnitTypeSetup {
                unit_type: x,
                kit: grunt(),
            },
            UnitTypeSetup {
                unit_type: y,
                kit: grunt(),
            },
            UnitTypeSetup {
                unit_type: tower_type,
                kit: UnitKit {
                    step: None,
                    ..grunt()
                },
            },
        ],
        heroes: vec![hero("hero-x", x), hero("hero-y", y)],
        spells: vec![spell],
    }
}

#[derive(Debug)]
struct Game {
    world: World,
    /// The one spell's ability.
    blink: AbilityId,
}

impl Game {
    /// A match of the test mode for 3 players, two on team `a` and one on `b`, with `limits`.
    fn new(script: &str, limits: ScriptLimits) -> Game {
        Game::start(script, limits).unwrap()
    }

    /// The match `new` gives; an error when the mode's start fails.
    fn start(script: &str, limits: ScriptLimits) -> Result<Game, CallError> {
        let scripts = MatchScripts { limits, players: 3 };
        let declared = [
            Capability::Combat,
            Capability::Navigation,
            Capability::Abilities,
        ];
        let TestMatch {
            mut world,
            mut schedule,
            mut registry,
        } = TestMatch::new(&declared, RATE, Some(scripts));
        let mut load = |name: &str, tag: &str| {
            let data = UnitTypeData {
                tags: vec![tag.to_owned()],
                params: BTreeMap::new(),
            };
            Units::load_type(&mut world, name, &data).unwrap()
        };
        let (grunt_type, tower_type) = (load("grunt", "grunt"), load("tower", "tower"));
        let (x, y) = (load("hero-x", "hero"), load("hero-y", "hero"));
        let blink = AbilityData {
            script: None,
            targeting: Targeting::None,
            range: None,
            cooldown_ms: None,
            cost: None,
            cast_time_ms: None,
            clamp_to_range: false,
            toggle: None,
            channel: None,
            hold: None,
            charges: None,
            charge: None,
            passive_modifier: None,
            passive_while_ready: false,
            projectile: None,
            area: None,
            params: BTreeMap::new(),
            projectile_state: BTreeMap::new(),
        };
        // A spell has one rank.
        let blink = Abilities::load(&mut world, &blink, None, 1).unwrap();
        let spell = SpellSetup {
            id: "blink".to_owned(),
            ability: blink,
        };
        let types = [grunt_type, tower_type, x, y];
        let files = mode_files();
        let script = Units::compile(&mut world, script).unwrap();
        let setup = setup(&files, script, types, spell);
        Mode::install(&mut world, &mut schedule, &mut registry, setup).unwrap();
        world.add_schedule(schedule);
        Mode::start(&mut world)?;
        Ok(Game { world, blink })
    }

    /// Runs a tick with `inputs`, each a player's slot and a mode input.
    fn tick(&mut self, inputs: &[(u32, ModeInput<'_>)]) {
        for (slot, input) in inputs {
            let payload = ModeInput::payload(slice::from_ref(input));
            self.world.resource_mut::<TickInputs>().push(TickInput {
                slot: PlayerSlot::new(*slot),
                payload: &payload,
            });
        }
        self.world.run_schedule(SimUpdate);
    }

    /// Unit `id`.
    fn entity(&self, id: u64) -> Entity {
        let mut units = self.world.resource::<EntityIndex>().iter();
        units.find(|(unit, _)| unit.get() == id).unwrap().1
    }

    /// The state fields `phase`, `seen`, `count` and `inputs`.
    fn state(&self) -> [StateValue; 4] {
        ["phase", "seen", "count", "inputs"].map(|name| self.field(name))
    }

    /// The state field `name`: the state holds the fields in the order of their names.
    fn field(&self, name: &str) -> StateValue {
        let book = self.world.non_send::<ModeCtx>();
        let at = book.book().schema.state_field(name).unwrap().index;
        self.world.resource::<ModeState>().get()[at].clone()
    }

    /// Each unit: its id, where it stands, its team, and whether it walks a lane from its start.
    fn units(&self) -> Vec<(u64, Position, u8, Option<PathDirection>)> {
        let world = &self.world;
        world
            .resource::<EntityIndex>()
            .iter()
            .map(|(id, entity)| {
                let unit = world.entity(entity);
                (
                    id.get(),
                    *unit.get::<Position>().unwrap(),
                    unit.get::<Team>().unwrap().index(),
                    unit.get::<LaneWalker>().map(|walker| walker.direction()),
                )
            })
            .collect()
    }

    /// The tick's failed calls: each the API's refusal, or `None` for another failure.
    fn failures(&self) -> Vec<Option<ApiError>> {
        let failures = self.world.non_send::<ScriptFailures>();
        failures
            .get()
            .iter()
            .map(|failure| match &failure.error {
                CallError::Api(error) => Some(*error),
                _ => None,
            })
            .collect()
    }
}

fn input<'a>(name: &'a str, value: &'a str) -> ModeInput<'a> {
    ModeInput {
        name,
        value: InputValue::String(value),
    }
}

/// `phase`, `seen`, `count` and `inputs`.
fn state(phase: &str, seen: i64, count: i64, inputs: i64) -> [StateValue; 4] {
    let [seen, count, inputs] = [seen, count, inputs].map(StateValue::Int);
    [StateValue::Text(phase.to_owned()), seen, count, inputs]
}

#[test]
fn the_start_spawns_the_map_then_runs_on_match_start_and_timers_never_fire_early() {
    let mut game = Game::new(SCRIPT, LIMITS);
    // Before tick 0: the map's tower, 0, then the match start's spawns in order: the neutral
    // grunt, 1, at the map's neutral spawn; team a's wave of two, 2 and 3, at the lane's start;
    // team b's wave of one, 4, at its end. Teams a and b are 0 and 1, neutral 2.
    assert_eq!(
        game.units(),
        [
            (0, at(-8, 0), 0, None),
            (1, at(0, 0), 2, None),
            (2, at(-10, 0), 0, Some(PathDirection::Forward)),
            (3, at(-10, 0), 0, Some(PathDirection::Forward)),
            (4, at(10, 0), 1, Some(PathDirection::Backward)),
        ]
    );
    let tower = game.entity(0);
    assert_eq!(
        game.world.get::<OnLane>(tower).map(|lane| lane.get()),
        Some(Lane::new(0))
    );
    // The map's grid is the match's, for its 3 teams: a, b and the neutral one.
    let vision = *game.world.resource::<VisionGrid>();
    assert_eq!((vision.grid, vision.teams), (map().grid.unwrap(), 3));
    assert_eq!(game.state(), state("start", 0, 0, 0));

    // Set at the start, time 0: "every" is due at 1, the end of tick 0, and every tick after;
    // "once", 250 ms, 2.5 ticks rounded up to 3, at the end of tick 2, with its data.
    let mut seen = Vec::new();
    for _ in 0..4 {
        game.tick(&[]);
        seen.push(game.state());
    }
    assert_eq!(
        seen,
        [
            state("start", 0, 1, 0),
            state("start", 0, 2, 0),
            state("start", 7, 3, 0),
            state("start", 7, 4, 0),
        ]
    );
    assert!(game.failures().is_empty());
}

#[test]
fn player_inputs_choose_heroes_and_spells_and_a_failed_call_changes_nothing() {
    let mut game = Game::new(SCRIPT, LIMITS);
    let spells = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["blink"]),
    };
    let twice = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["blink", "blink"]),
    };
    let wrong_type = ModeInput {
        name: "hero",
        value: InputValue::StringList(vec!["hero-x"]),
    };
    game.tick(&[
        (0, spells),
        (0, input("hero", "hero-x")),
        // Taken by player 0.
        (2, input("hero", "hero-x")),
        (2, twice),
        (2, input("hero", "hero-y")),
        // Neither of the mode's inputs: never a call.
        (2, input("nope", "x")),
        (2, wrong_type),
        (1, input("fail", "boom")),
        (1, input("phase", "")),
    ]);
    // The failed calls count no input and spawn nothing: three calls succeeded, player 0's two
    // and player 2's choice of the other hero. The thrown call fails with no refusal of the API.
    assert_eq!(game.field("inputs"), StateValue::Int(3));
    assert_eq!(game.field("phase"), StateValue::Text("start".to_owned()));
    assert_eq!(
        game.failures(),
        [
            Some(ApiError::HeroTaken),
            Some(ApiError::RepeatedSpell),
            None,
            Some(ApiError::WrongStateType)
        ]
    );
    let picks = game.world.resource::<Picks>().get().to_vec();
    assert_eq!(
        picks,
        [
            Pick {
                hero: Some(HeroIndex::new(0)),
                spells: vec![SpellIndex::new(0)],
                spawned: true,
            },
            Pick::default(),
            Pick {
                hero: Some(HeroIndex::new(1)),
                spells: Vec::new(),
                spawned: true,
            },
        ]
    );
    // The heroes, 5 and 6, at their teams' spawns under their players' control: player 0's
    // with its spell learned.
    let heroes: Vec<_> = game.units()[5..].to_vec();
    assert_eq!(heroes, [(5, at(0, -5), 0, None), (6, at(0, 5), 1, None)]);
    let hero = game.entity(5);
    assert_eq!(
        game.world.get::<Owner>(hero).unwrap().slot(),
        PlayerSlot::new(0)
    );
    let slots = game.world.get::<AbilitySlots>(hero).unwrap();
    assert_eq!(
        slots.slot(0).map(|slot| (slot.ability, slot.rank)),
        Some((game.blink, 1))
    );
}

#[test]
fn resources_add_up_and_queries_see_teams_lanes_and_the_dead() {
    let mut game = Game::new(SCRIPT, LIMITS);
    game.tick(&[(0, input("hero", "hero-x")), (2, input("hero", "hero-y"))]);
    // A dead grunt is still one the mode sees.
    let grunt = game.entity(3);
    game.world.entity_mut(grunt).insert(Dead);
    game.tick(&[
        (1, input("gold", "gold")),
        (1, input("rich", "gems")),
        (1, input("rich", "gems")),
        (0, input("probe", "")),
    ]);
    let resources = game.world.resource::<PlayerResources>();
    assert_eq!(resources.amount(PlayerSlot::new(1), "gold"), 16);
    assert_eq!(resources.amount(PlayerSlot::new(1), "gems"), i64::MAX);
    assert_eq!(resources.amount(PlayerSlot::new(0), "gold"), 0);
    assert_eq!(game.failures(), [Some(ApiError::ResourceOverflow)]);
    // The enemy of a, the 4 grunts with the dead one, b's one hero, the 2 playing teams, the 3
    // players, the lane and team of grunt 2, the neutral grunt 1's team, hero 5's owner, the lane of the tower,
    // which stands on it as grunt 2 walks it, and grunt 2's unit type.
    let text = |text: &str| StateValue::Text(text.to_owned());
    let seen = [
        "enemy",
        "grunts",
        "heroes",
        "teams",
        "players",
        "lane",
        "team",
        "neutral",
        "owner",
        "tower_lane",
        "kind",
    ]
    .map(|name| game.field(name));
    assert_eq!(
        seen,
        [
            text("b"),
            StateValue::Int(4),
            StateValue::Int(1),
            StateValue::Int(2),
            StateValue::Int(3),
            text("mid"),
            text("a"),
            text("neutral"),
            StateValue::Int(0),
            text("mid"),
            text("grunt"),
        ]
    );
}

#[test]
fn a_player_spends_only_their_own_pool() {
    // Each player's pool holds one whole call of 1000: player 0's spin runs its 1000 and fails,
    // which spends its pool, so its next input fails unrun; player 1's input in the same tick
    // runs from its own pool.
    let spin = r#"
fn on_mode_input(ctx, player, name, value) {
    ctx.state.inputs += 1;
    if name == "fail" {
        loop {}
    }
}
"#;
    let limits = ScriptLimits {
        per_call: 1000,
        player: 1000,
        ..LIMITS
    };
    let mut game = Game::new(spin, limits);
    let inputs = |game: &Game| game.field("inputs");
    game.tick(&[
        (0, input("fail", "")),
        (0, input("phase", "")),
        (1, input("phase", "")),
    ]);
    let failures = game.world.non_send::<ScriptFailures>();
    let errors: Vec<_> = failures
        .get()
        .iter()
        .map(|failure| &failure.error)
        .collect();
    assert!(
        matches!(
            errors[..],
            [
                CallError::Script(ScriptError::CallLimit),
                CallError::Script(ScriptError::TickBudget)
            ]
        ),
        "{errors:?}"
    );
    assert_eq!(inputs(&game), StateValue::Int(1));
    // Every pool starts full in the next tick.
    game.tick(&[(0, input("phase", ""))]);
    assert_eq!(inputs(&game), StateValue::Int(2));
}

#[test]
fn a_mode_whose_start_fails_starts_no_match() {
    let failing = "fn on_match_start(ctx) { ctx.spawn_unit(\"ghost\", \"a\", ctx.map.neutral_spawns[0].pos); }";
    let failed = Game::start(failing, LIMITS).err();
    assert!(
        matches!(failed, Some(CallError::Api(ApiError::UnknownUnitType))),
        "{failed:?}"
    );
}

#[test]
fn a_timer_whose_call_finds_the_mode_pool_spent_stays_due() {
    // A pool of 1500 operations: the first spinning call runs its 1000 and fails, which leaves
    // 500; the second ends past those, and its timer waits for the next tick.
    let spin = r#"
fn on_match_start(ctx) {
    ctx.timer("a", 100, false, ());
    ctx.timer("b", 100, false, ());
}

fn on_timer(ctx, name, data) {
    ctx.state.count += 1;
    loop {}
}
"#;
    let limits = ScriptLimits {
        per_call: 1000,
        mode: 1500,
        ..LIMITS
    };
    let mut game = Game::new(spin, limits);
    let due = |game: &Game| {
        let timers = game.world.resource::<Timers>();
        (
            timers
                .due(Tick::new(u64::MAX))
                .map(|timer| timer.name.clone()),
            timers.due(Tick::ZERO).is_some(),
        )
    };
    game.tick(&[]);
    assert_eq!(due(&game), (Some("b".to_owned()), false));
    game.tick(&[]);
    assert_eq!(due(&game), (None, false));
    // Both calls failed, so none counted.
    assert_eq!(game.field("count"), StateValue::Int(0));
}

#[test]
fn a_death_reaches_the_mode_and_a_respawn_brings_the_unit_back_at_its_spawn() {
    // The mode spawns as the test mode does, records each death, and respawns the dead 250 ms
    // later: 3 ticks at 10 a second. A probe respawns the first unit with the tag it names.
    let script = r#"
fn on_match_start(ctx) {
    for point in ctx.map.neutral_spawns {
        ctx.spawn_unit(point.unit_type, "neutral", point.pos);
    }
    ctx.spawn_wave("a", "mid", ctx.p.wave);
    ctx.spawn_wave("b", "mid", ["grunt"]);
}

fn on_unit_died(ctx, unit, killer, assisters) {
    ctx.state.count += 1;
    ctx.state.kind = unit.unit_type;
    ctx.state.team = killer.team;
    ctx.state.grunts = assisters.len();
    ctx.state.lane = assisters[0].team;
    ctx.respawn(unit, 250);
}

fn on_mode_input(ctx, player, name, value) {
    ctx.respawn(ctx.units_tagged(value)[0], 100);
}
"#;
    let mut game = Game::new(script, LIMITS);
    // Units: the tower 0 of a, the neutral grunt 1 at (0, 0), a's grunts 2 and 3, b's grunt 4. The
    // neutral grunt stands at (3, 0) when b's grunt strikes it for its 10 health, in tick 0; a's
    // grunt 2 struck it in the same tick, within the window of 10 ticks.
    let victim = game.entity(1);
    *game.world.get_mut::<Position>(victim).unwrap() = at(3, 0);
    game.world.insert_resource(AssistWindow(Ticks::new(10)));
    let ids: Vec<_> = game
        .world
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, _)| id)
        .collect();
    let (one, two, four) = (ids[1], ids[2], ids[4]);
    game.world
        .resource_scope(|world, index: Mut<'_, EntityIndex>| {
            let mut attackers = world.get_mut::<RecentAttackers>(victim).unwrap();
            attackers.record(two, Tick::new(0), &index);
        });
    game.world.resource_mut::<Strikes>().0.push(Strike {
        source: four,
        target: one,
        amount: num(10),
    });
    game.tick(&[]);
    // It died in tick 0 with killer 4 of b and one assister of a: the mode set its respawn for the
    // end of tick 0, 1, plus 3 ticks: the start of tick 4.
    let seen = ["count", "kind", "team", "grunts", "lane"].map(|name| game.field(name));
    let text = |text: &str| StateValue::Text(text.to_owned());
    assert_eq!(
        seen,
        [
            StateValue::Int(1),
            text("grunt"),
            text("b"),
            StateValue::Int(1),
            text("a"),
        ]
    );
    assert_eq!(
        game.world.get::<Respawn>(victim),
        Some(&Respawn { at: Tick::new(4) })
    );
    for _ in 1..4 {
        game.tick(&[]);
        assert!(game.world.entity(victim).contains::<Dead>());
    }
    game.tick(&[]);
    assert!(!game.world.entity(victim).contains::<Dead>());
    assert_eq!(game.world.get::<Position>(victim), Some(&at(0, 0)));
    assert_eq!(game.world.get::<Health>(victim).unwrap().current(), num(10));

    // A living unit, and a dead one whose type despawns, cannot respawn.
    game.tick(&[(0, input("probe", "tower"))]);
    assert_eq!(game.failures(), [Some(ApiError::RespawnAlive)]);
    let tower = game.entity(0);
    game.world
        .entity_mut(tower)
        .insert((Dead, OnDeath::Despawn));
    game.tick(&[(0, input("probe", "tower"))]);
    assert_eq!(game.failures(), [Some(ApiError::RespawnDespawns)]);
    assert!(game.world.get_entity(tower).is_err());
}
