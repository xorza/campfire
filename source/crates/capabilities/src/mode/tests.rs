use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::slice;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use campfire_content::PackagePath;
use campfire_math::{Num, Vec3};
use campfire_script::{Budget, ScriptId};
use campfire_sim::{Capability, IdAllocator, SimUpdate, StableId, TickInput};

use super::*;
use crate::abilities::Abilities;
use crate::abilities::ability_book::AbilityId;
use crate::abilities::ability_data::{AbilityData, Targeting};
use crate::abilities::resource_pool::ResourcePool;
use crate::capability_set::internals::TestMatch;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_kind::DamageKind;
use crate::combat::damage_queue::DamageQueue;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::mode::avatar_index::AvatarIndex;
use crate::mode::loadout_index::LoadoutIndex;
use crate::mode::map_data::{GridData, NeutralSpawnData, PathData, StructureData};
use crate::mode::match_end::MatchResult;
use crate::mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
use crate::mode::mode_setup::{AvatarSetup, LoadoutSetup, UnitTypeSetup};
use crate::mode::unit_kit::UnitKit;
use crate::navigation::destination::Destination;
use crate::navigation::move_step::MoveStep;
use crate::navigation::path_walker::{PathDirection, PathWalker};
use crate::scripts::error::ApiError;
use crate::scripts::hook::ScriptRole;
use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::state_decl::{StateDecl, StateDefault, StateType, SyncTo};
use crate::scripts::state_value::StateValue;
use crate::stats::level::Level;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifiers::Modifiers;
use crate::stats::modifiers::{Application, Instance, StatShare};
use crate::stats::stat::Stat;
use crate::stats::stat_rule::{Combine, StatRule};
use crate::stats::stats_data::StatsData;
use crate::stats::unit_stats::UnitStats;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::tag_set::TagSet;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::bounds::Bounds;
use crate::values::declared_name::DeclaredName;
use crate::values::grid::Grid;
use crate::values::number::Number;
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
/// The reference MOBA's damage kinds, and the stats its `calc_damage` reads.
const DAMAGE_KINDS: [&str; 3] = ["physical", "magic", "true"];
const STATS_3V3: [&str; 8] = [
    "armor",
    "armor_pen",
    "armor_pen_pct",
    "damage_dealt_pct",
    "magic_pen",
    "magic_pen_pct",
    "magic_resist",
    "physical_block",
];

const SCRIPT: &str = r#"
fn on_match_start(ctx) {
    ctx.state.phase = "start";
    ctx.timer("once", 250, false, 7);
    ctx.timer("every", 100, true, ());
    for point in ctx.map.neutral_spawns {
        ctx.spawn_unit(point.unit_type, "neutral", point.pos);
    }
    ctx.spawn_group("a", "mid", ctx.p.group);
    ctx.spawn_group("b", "mid", ["grunt"]);
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
        ctx.choose_avatar(player, value);
        ctx.spawn_avatars();
    } else if name == "spells" {
        ctx.choose_loadout(player, value);
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
        ctx.state.heroes = ctx.avatars("b").len();
        ctx.state.teams = ctx.teams.len();
        ctx.state.players = ctx.players;
        ctx.state.path = grunts[1].path;
        ctx.state.team = grunts[1].team;
        ctx.state.neutral = grunts[0].team;
        ctx.state.owner = ctx.avatars()[0].owner;
        ctx.state.tower_path = ctx.units_tagged("tower")[0].path;
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
        body: None,
    }
}

/// Bounds from (−10, −5) to (10, 6) with a grid of 1 m cells; one path, `mid`, along x; team a's spawn at
/// z = −5 and b's at 5; a's tower 8 m down the path; and a neutral grunt in the middle.
fn map() -> MapData {
    MapData {
        bounds: Bounds::new([num(-10), num(-5)], [num(10), num(6)]).unwrap(),
        grid: Some(GridData {
            cell: Scalar::Int(1),
        }),
        navigation: Some(GridData {
            cell: Scalar::Int(1),
        }),
        paths: vec![PathData {
            name: "mid".to_owned(),
            points: vec![point(-10, 0), point(0, 0), point(10, 0)],
        }],
        spawns: [("a", point(0, -5)), ("b", point(0, 5))]
            .map(|(team, point)| (team.to_owned(), point))
            .into(),
        structures: vec![StructureData {
            unit_type: "tower".to_owned(),
            team: "a".to_owned(),
            path: Some("mid".to_owned()),
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

/// The mode's one modifier: 200 ms, stacking up to 4, with a count in its state.
fn blessing() -> ModifierData {
    ModifierData {
        script: None,
        duration_ms: Some(Number::Value(Scalar::Int(200))),
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Stack,
        max_stacks: NonZeroU32::new(4),
        stats: BTreeMap::new(),
        tags: Vec::new(),
        shield: None,
        aura: None,
        params: BTreeMap::new(),
        state: [("count".to_owned(), field(StateType::Int, None))].into(),
    }
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
                ("path", field(StateType::String, None)),
                ("team", field(StateType::String, None)),
                ("neutral", field(StateType::String, None)),
                ("owner", field(StateType::Int, None)),
                ("tower_path", field(StateType::String, None)),
                ("kind", field(StateType::String, None)),
            ]
            .map(|(name, decl)| (name.to_owned(), decl))
            .into(),
            params: [
                ("group", ModeParam::List(vec![text("grunt"), text("grunt")])),
                ("gold", ModeParam::Value(Scalar::Int(8))),
            ]
            .map(|(name, param)| (name.to_owned(), param))
            .into(),
            modifiers: [("blessing".to_owned(), blessing())].into(),
            damage_kinds: DAMAGE_KINDS
                .map(|kind| DeclaredName::new(kind).unwrap())
                .into(),
            attack_kind: None,
            stats: STATS_3V3
                .map(|name| {
                    let sum = StatRule {
                        combine: Combine::Sum,
                        min: None,
                        max: None,
                    };
                    (Stat::named(name).unwrap(), sum)
                })
                .into(),
            resources: Vec::new(),
            tags: BTreeMap::new(),
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
/// its one spell, and hero X's one ability, `strike`.
fn setup(
    files: &ModeFiles,
    script: ScriptId,
    types: [UnitType; 4],
    spell: LoadoutSetup,
    strike: AbilityId,
    blessing: ModifierId,
) -> ModeSetup<'_> {
    let [grunt_type, tower_type, x, y] = types;
    let hero = |id: &str, unit_type, abilities, passive| AvatarSetup {
        passive,
        id: id.to_owned(),
        unit_type,
        abilities,
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
                stats: StatsData::default(),
            },
            UnitTypeSetup {
                unit_type: x,
                kit: grunt(),
                stats: StatsData::default(),
            },
            UnitTypeSetup {
                unit_type: y,
                kit: grunt(),
                stats: StatsData::default(),
            },
            UnitTypeSetup {
                unit_type: tower_type,
                kit: UnitKit {
                    step: None,
                    ..grunt()
                },
                stats: StatsData::default(),
            },
        ],
        avatars: vec![
            hero("hero-x", x, vec![strike], None),
            hero("hero-y", y, Vec::new(), Some(blessing)),
        ],
        loadout: vec![spell],
        walkers: vec![Body::radius_of(grunt().body.as_ref())],
        max_move_speed: num(10),
    }
}

#[derive(Debug)]
struct Game {
    world: World,
    /// The one spell's ability.
    blink: AbilityId,
    /// Hero X's ability, of 2 ranks.
    strike: AbilityId,
}

impl Game {
    /// A match of the test mode for 3 players, two on team `a` and one on `b`, with `limits`.
    fn new(script: &str, limits: ScriptLimits) -> Game {
        Game::start(script, limits).unwrap()
    }

    /// The match `new` gives; an error when the mode's start fails.
    fn start(script: &str, limits: ScriptLimits) -> Result<Game, CallError> {
        let scripts = MatchScripts {
            limits,
            players: 3,
            damage_kinds: DAMAGE_KINDS
                .map(|kind| DeclaredName::new(kind).unwrap())
                .into(),
        };
        let declared = [
            Capability::Stats,
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
        let (x, y) = (load("hero-x", "avatar"), load("hero-y", "avatar"));
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
        // A spell has one rank; hero X's ability, 2.
        let strike = Abilities::load(&mut world, 0, "strike", &blink, None, 2).unwrap();
        let blink = Abilities::load(&mut world, 0, "blink", &blink, None, 1).unwrap();
        let spell = LoadoutSetup {
            id: "blink".to_owned(),
            ability: blink,
        };
        let types = [grunt_type, tower_type, x, y];
        let files = mode_files();
        for (name, data) in &files.data.modifiers {
            Stats::load_modifier(&mut world, 0, name, data, None);
        }
        let script = Units::compile(&mut world, script).unwrap();
        let blessing = Stats::modifier(&world, 0, "blessing").unwrap();
        let setup = setup(&files, script, types, spell, strike, blessing);
        Mode::install(&mut world, &mut schedule, &mut registry, setup).unwrap();
        world.add_schedule(schedule);
        Mode::start(&mut world)?;
        Ok(Game {
            world,
            blink,
            strike,
        })
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
        let ctx = self.world.non_send::<Ctx>();
        let at = ctx.mode().unwrap().schema.state_field(name).unwrap().index;
        self.world.resource::<ModeState>().get()[at].clone()
    }

    /// Each unit: its id, where it stands, its team, and whether it walks a path from its start.
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
                    unit.get::<PathWalker>().map(|walker| walker.direction()),
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
    // grunt, 1, at the map's neutral spawn; team a's spawn group of two, 2 and 3, at the path's start;
    // team b's spawn group of one, 4, at its end. Teams a and b are 0 and 1, neutral 2.
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
        game.world.get::<OnPath>(tower).map(|path| path.get()),
        Some(PathId::new(0))
    );
    // The map's grid is the match's, for its 3 teams: a, b and the neutral one.
    let vision = *game.world.resource::<VisionGrid>();
    let grid = Grid::new(num(1), map().bounds).unwrap();
    assert_eq!((vision.grid, vision.teams), (grid, 3));
    assert_eq!(*game.world.resource::<Bounds>(), map().bounds);
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
            Some(ApiError::AvatarTaken),
            Some(ApiError::RepeatedLoadout),
            None,
            Some(ApiError::WrongStateType)
        ]
    );
    let picks = game.world.resource::<Picks>().get().to_vec();
    assert_eq!(
        picks,
        [
            Pick {
                avatar: Some(AvatarIndex::new(0)),
                loadout: vec![LoadoutIndex::new(0)],
                spawned: true,
            },
            Pick::default(),
            Pick {
                avatar: Some(AvatarIndex::new(1)),
                loadout: Vec::new(),
                spawned: true,
            },
        ]
    );
    // The heroes, 5 and 6, at their teams' spawns under their players' control: player 0's
    // with its own ability unlearned, then its spell learned.
    let heroes: Vec<_> = game.units()[5..].to_vec();
    assert_eq!(heroes, [(5, at(0, -5), 0, None), (6, at(0, 5), 1, None)]);
    let hero = game.entity(5);
    assert_eq!(
        game.world.get::<Owner>(hero).unwrap().slot(),
        PlayerSlot::new(0)
    );
    let slots = game.world.get::<AbilitySlots>(hero).unwrap();
    let slots: Vec<_> = slots.iter().map(|slot| (slot.ability, slot.rank)).collect();
    assert_eq!(slots, [(game.strike, 0), (game.blink, 1)]);
}

#[test]
fn a_mode_learns_a_hero_ability_up_to_its_last_rank_and_a_failed_call_learns_nothing() {
    // Hero X holds its ability in slot 0, of 2 ranks, and the spell in slot 1, of 1.
    let learner = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "spells" {
        ctx.choose_loadout(player, value);
        return;
    }
    if name == "hero" {
        ctx.choose_avatar(player, value);
        ctx.spawn_avatars();
        return;
    }
    let hero = ctx.avatars()[0];
    let slot = if value == "spell" { 1 } else if value == "none" { 2 } else if value == "negative" { -1 } else { 0 };
    let times = if value == "twice" { 2 } else if value == "thrice" { 3 } else { 1 };
    for time in 0..times {
        ctx.learn(hero, slot);
    }
}
"#;
    let mut game = Game::new(learner, LIMITS);
    let spells = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["blink"]),
    };
    game.tick(&[(0, spells), (0, input("hero", "hero-x"))]);
    let mut owned = game.world.query_filtered::<Entity, With<Owner>>();
    let hero = owned.single(&game.world).unwrap();
    let ranks = |game: &Game| {
        let slots = game.world.get::<AbilitySlots>(hero).unwrap();
        slots.iter().map(|slot| slot.rank).collect::<Vec<_>>()
    };
    // Three ranks of two fail at the third, and the call learns none; two in one call count the
    // first queued, and reach the last rank; then neither slot has a rank more, and two slots do
    // not exist.
    let steps = [
        ("thrice", [0, 1], Some(ApiError::MaxRank)),
        ("twice", [2, 1], None),
        ("once", [2, 1], Some(ApiError::MaxRank)),
        ("spell", [2, 1], Some(ApiError::MaxRank)),
        ("none", [2, 1], Some(ApiError::NoAbilitySlot)),
        ("negative", [2, 1], Some(ApiError::NoAbilitySlot)),
    ];
    for (value, expected, failure) in steps {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(ranks(&game), expected, "{value}");
        let failures: Vec<_> = failure.into_iter().map(Some).collect();
        assert_eq!(game.failures(), failures, "{value}");
    }
}

#[test]
fn a_mode_applies_a_modifier_writes_its_handle_and_sees_it_end() {
    let blesser = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        ctx.choose_avatar(player, value);
        ctx.spawn_avatars();
        return;
    }
    let hero = ctx.avatars()[0];
    if value == "bless" {
        let m = ctx.add_modifier(hero, "blessing", 100);
        ctx.state.seen = m.stacks;
        m.stacks = 3;
        m.state.count = m.state.count + 5;
    } else if value == "again" {
        ctx.state.seen = ctx.add_modifier(hero, "blessing").stacks;
    } else if value == "check" {
        ctx.state.seen = if hero.has_modifier("blessing") { 1 } else { 0 };
    } else if value == "twice" {
        let first = ctx.add_modifier(hero, "blessing");
        let second = ctx.add_modifier(hero, "blessing");
        ctx.state.seen = first.stacks * 10 + second.stacks;
    } else if value == "renew" {
        let m = ctx.add_modifier(hero, "blessing");
        m.state.count = 9;
        ctx.remove(m);
        let renewed = ctx.add_modifier(hero, "blessing");
        ctx.state.seen = renewed.stacks * 10 + renewed.state.count;
    } else if value == "unknown" {
        ctx.add_modifier(hero, "curse");
    }
}
"#;
    let mut game = Game::new(blesser, LIMITS);
    game.tick(&[(0, input("hero", "hero-x"))]);
    let mut owned = game.world.query_filtered::<Entity, With<Owner>>();
    let hero = owned.single(&game.world).unwrap();
    let held = |game: &Game| {
        let modifiers = game.world.get::<Modifiers>(hero).unwrap();
        modifiers
            .iter()
            .map(|instance| {
                (
                    instance.stacks,
                    instance.until.map(Tick::get),
                    instance.state.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    // In tick t a new one, 1 stack as the call sees it, written to 3 and its count from 0 to 5;
    // 100 ms at 10 ticks a second is 1 tick, so it holds through t + 1 and ends as t + 2
    // starts.
    let t = game.world.resource::<SimTick>().start().get();
    game.tick(&[(0, input("probe", "bless"))]);
    assert_eq!(game.field("seen"), StateValue::Int(1));
    assert_eq!(held(&game), [(3, Some(t + 2), vec![StateValue::Int(5)])]);
    // Again in t + 1 with its own 200 ms, 2 ticks: a fourth stack, the limit, and the end
    // t + 1 + 2 + 1; the count stays.
    game.tick(&[(0, input("probe", "again"))]);
    assert_eq!(game.field("seen"), StateValue::Int(4));
    assert_eq!(held(&game), [(4, Some(t + 4), vec![StateValue::Int(5)])]);
    game.tick(&[(0, input("probe", "check"))]);
    assert_eq!(game.field("seen"), StateValue::Int(1));
    // Through t + 3, then gone as t + 4 starts: once that tick has run.
    while game.world.resource::<SimTick>().start().get() <= t + 4 {
        game.tick(&[]);
    }
    assert_eq!(held(&game), []);
    game.tick(&[(0, input("probe", "check"))]);
    assert_eq!(game.field("seen"), StateValue::Int(0));
    // Two applications in one call in tick u: one handle, which sees both, 2 stacks; 200 ms, so
    // it ends as u + 3 starts.
    let u = game.world.resource::<SimTick>().start().get();
    game.tick(&[(0, input("probe", "twice"))]);
    assert_eq!(game.field("seen"), StateValue::Int(22));
    assert_eq!(held(&game), [(2, Some(u + 3), vec![StateValue::Int(0)])]);
    // In u + 1, a third stack written, removed, and applied again: a new one, 1 stack and its
    // count 0, which ends as u + 4 starts.
    game.tick(&[(0, input("probe", "renew"))]);
    assert_eq!(game.field("seen"), StateValue::Int(10));
    assert_eq!(held(&game), [(1, Some(u + 4), vec![StateValue::Int(0)])]);
    game.tick(&[(0, input("probe", "unknown"))]);
    assert_eq!(game.failures(), [Some(ApiError::UnknownModifier)]);
}

#[test]
fn resources_add_up_and_queries_see_teams_paths_and_the_dead() {
    let mut game = Game::new(SCRIPT, LIMITS);
    game.tick(&[(0, input("hero", "hero-x")), (2, input("hero", "hero-y"))]);
    // Hero Y carries its passive, the blessing, from itself, with no end.
    let mut owned = game.world.query::<(&StableId, &Owner, &Modifiers)>();
    let (&hero_y, _, modifiers) = owned
        .iter(&game.world)
        .find(|(_, owner, _)| owner.slot() == PlayerSlot::new(2))
        .unwrap();
    let held: Vec<_> = modifiers
        .iter()
        .map(|instance| {
            (
                instance.source,
                instance.passive,
                instance.until,
                instance.stacks,
            )
        })
        .collect();
    assert_eq!(held, [(Some(hero_y), true, None, 1)]);
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
    // players, the path and team of grunt 2, the neutral grunt 1's team, hero 5's owner, the path of the tower,
    // which stands on it as grunt 2 walks it, and grunt 2's unit type.
    let text = |text: &str| StateValue::Text(text.to_owned());
    let seen = [
        "enemy",
        "grunts",
        "heroes",
        "teams",
        "players",
        "path",
        "team",
        "neutral",
        "owner",
        "tower_path",
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
    ctx.spawn_group("a", "mid", ctx.p.group);
    ctx.spawn_group("b", "mid", ["grunt"]);
}

fn on_unit_died(ctx, unit, killer, assisters) {
    ctx.state.count += 1;
    ctx.state.kind = unit.unit_type;
    ctx.state.team = killer.team;
    ctx.state.grunts = assisters.len();
    ctx.state.path = assisters[0].team;
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
    game.world.resource_mut::<DamageQueue>().push(Damage {
        source: Some(four),
        target: one,
        amount: num(10),
        kind: DamageKind::new(0),
        cause: DamageCause::Effect,
        ability: None,
        depth: 0,
    });
    game.tick(&[]);
    // It died in tick 0 with killer 4 of b and one assister of a: the mode set its respawn for the
    // end of tick 0, 1, plus 3 ticks: the start of tick 4.
    let seen = ["count", "kind", "team", "grunts", "path"].map(|name| game.field(name));
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

#[test]
fn a_match_ends_once_and_then_no_stage_runs() {
    // A timer counts every tick; inputs end the match.
    let script = r#"
fn on_match_start(ctx) {
    ctx.timer("every", 100, true, ());
    ctx.spawn_group("a", "mid", ["grunt"]);
}

fn on_timer(ctx, name, data) {
    ctx.state.count += 1;
}

fn on_mode_input(ctx, player, name, value) {
    if name == "probe" {
        ctx.end(value);
        ctx.end(value);
    } else if name == "hero" {
        ctx.end(value);
    } else {
        ctx.end(());
    }
}
"#;
    let mut game = Game::new(script, LIMITS);
    // A second end in the same call fails the call, which ends nothing; so does a team the mode
    // does not have. The timer fires at the end of ticks 0 and 1.
    game.tick(&[(0, input("probe", "a"))]);
    assert_eq!(game.failures(), [Some(ApiError::Ended)]);
    game.tick(&[(0, input("hero", "z"))]);
    assert_eq!(game.failures(), [Some(ApiError::UnknownTeam)]);
    assert!(!game.world.contains_resource::<MatchEnd>());
    assert_eq!(game.field("count"), StateValue::Int(2));

    // Team b wins in the Inputs stage of tick 2, so no later stage of tick 2 runs: a's grunt,
    // 1, sent 5 m away, stands where it is, and the timer counts no more. In tick 3 nothing
    // runs, the input to end again included.
    let grunt = game.entity(1);
    let mut destination = game.world.get_mut::<Destination>(grunt).unwrap();
    destination.set(Some(at(5, 0)));
    let before = game.units();
    game.tick(&[(0, input("hero", "b"))]);
    let end = MatchEnd::new(Tick::new(2), MatchResult::Won(Team::new(1)));
    assert_eq!(game.world.get_resource::<MatchEnd>(), Some(&end));
    game.tick(&[(0, input("phase", "draw"))]);
    assert_eq!(game.failures(), []);
    assert_eq!(game.world.get_resource::<MatchEnd>(), Some(&end));
    assert_eq!(game.units(), before);
    assert_eq!(game.field("count"), StateValue::Int(2));
    assert_eq!(game.world.resource::<SimTick>().start(), Tick::new(4));

    // `end(())` is a draw.
    let mut game = Game::new(script, LIMITS);
    game.tick(&[(0, input("phase", "draw"))]);
    let draw = MatchEnd::new(Tick::new(0), MatchResult::Draw);
    assert_eq!(game.world.get_resource::<MatchEnd>(), Some(&draw));
}

/// The reference 3v3's `calc_damage` and the function it calls, as its package holds them.
fn calc_damage_3v3() -> &'static str {
    const MODE_3V3: &str = include_str!("../../../../packages/moba/modes/3v3/scripts/mode.rhai");
    let start = MODE_3V3.find("// A source that is gone").unwrap();
    let body = MODE_3V3.find("fn calc_damage(ctx, d) {").unwrap();
    let end = body + MODE_3V3[body..].find("\n}\n").unwrap() + 3;
    &MODE_3V3[start..end]
}

impl Game {
    /// A grunt of 1000 health on `team`, whose modifier adds `stats` by name.
    fn fighter(&mut self, team: u8, stats: &[(&str, Num)]) -> StableId {
        let book = self.world.resource::<StatBook>();
        let shares = stats.iter().map(|&(name, value)| StatShare {
            stat: book.index(&Stat::named(name).unwrap()).unwrap(),
            value,
        });
        let instance = Instance {
            id: ModifierId::new(0),
            source: None,
            ability: None,
            rank: 1,
            passive: false,
            aura: false,
            aura_radius: None,
            stacks: 1,
            until: None,
            stack_life: None,
            stack_ends: Vec::new(),
            interval: None,
            shield: None,
            stats: shares.collect(),
            tags: TagSet::default(),
            state: vec![StateValue::Int(0)],
        };
        let mut modifiers = Modifiers::default();
        modifiers.apply(Application {
            instance,
            reapply: Reapply::Refresh,
            max_stacks: None,
        });
        let grunt = self.world.non_send::<View>().unit_type("grunt").unwrap();
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world.spawn((
            id,
            at(0, 0),
            Team::new(team),
            Health::new(num(1000)).unwrap(),
            grunt,
            Level::default(),
            UnitStats::default(),
            modifiers,
        ));
        id
    }

    /// Queues `amount` of damage of the kind `kind` from `source` to `target`, dealt by `cause`.
    fn damage(
        &mut self,
        source: Option<StableId>,
        target: StableId,
        amount: i64,
        kind: &str,
        cause: DamageCause,
    ) {
        let kind = DAMAGE_KINDS.iter().position(|&name| name == kind).unwrap();
        self.world.resource_mut::<DamageQueue>().push(Damage {
            source,
            target,
            amount: num(amount),
            kind: DamageKind::new(u8::try_from(kind).unwrap()),
            cause,
            ability: None,
            depth: 0,
        });
    }

    fn health(&self, id: StableId) -> Num {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world.get::<Health>(entity).unwrap().current()
    }
}

#[test]
fn the_3v3s_calc_damage_weighs_each_hit_exactly() {
    let mut game = Game::new(calc_damage_3v3(), LIMITS);
    let half = Num::ONE / 2;
    // The source deals 50% more, ignores half of armor, then 10 more.
    let source = game.fighter(
        0,
        &[
            ("damage_dealt_pct", half),
            ("armor_pen_pct", half),
            ("armor_pen", num(10)),
        ],
    );
    let armored = game.fighter(
        1,
        &[
            ("armor", num(120)),
            ("magic_resist", num(60)),
            ("physical_block", num(5)),
        ],
    );
    let exposed = game.fighter(1, &[("armor", num(-100))]);
    game.tick(&[]);
    let crit = DamageCause::Attack { crit: true };
    let attack = DamageCause::Attack { crit: false };
    // 100 physical, 150 dealt: armor 120 × (1 − 0.5) − 10 = 50, 150 × 100 ÷ 150 = 100, less the
    // block of 5: 95.
    game.damage(Some(source), armored, 100, "physical", attack);
    // 40 magic, a crit: 40 × 1.5 × 2 = 120, magic resist 60 with no magic pen: 120 × 100 ÷ 160
    // = 75, no block.
    game.damage(Some(source), armored, 40, "magic", crit);
    // 100 physical, 150 dealt, against armor −100, which pen does not touch: 150 × (2 − 100 ÷
    // 200) = 225.
    game.damage(Some(source), exposed, 100, "physical", attack);
    // 100 true, 150 dealt, whatever the armor.
    game.damage(Some(source), exposed, 100, "true", DamageCause::Effect);
    // 100 physical from no source: no bonus, 100 × 1.5 = 150.
    game.damage(None, exposed, 100, "physical", DamageCause::Effect);
    game.tick(&[]);
    assert_eq!(game.failures(), []);
    assert_eq!(game.health(armored), num(1000 - 95 - 75));
    assert_eq!(game.health(exposed), num(1000 - 225 - 150 - 150));
}

#[test]
fn calc_damage_is_pure_outside_the_pools_and_a_failure_keeps_the_amount() {
    let script = r#"
fn calc_damage(ctx, d) {
    if d.kind == "magic" {
        ctx.timer("late", 100, false, ());
    }
    if d.kind == "true" {
        return "none";
    }
    d.amount * 3
}
"#;
    // A mode pool of one operation, which no call of `calc_damage` draws from.
    let limits = ScriptLimits { mode: 1, ..LIMITS };
    let mut game = Game::new(script, limits);
    let source = game.fighter(0, &[]);
    let target = game.fighter(1, &[]);
    game.tick(&[]);
    // Physical 10, tripled; magic 10, whose timer the pure `ctx` refuses; true 10, which returns
    // no number. The two failures keep their 10: 1000 − 30 − 10 − 10.
    for kind in DAMAGE_KINDS {
        game.damage(Some(source), target, 10, kind, DamageCause::Effect);
    }
    game.tick(&[]);
    assert_eq!(
        game.failures(),
        [Some(ApiError::PureCall), Some(ApiError::NotAnAmount)]
    );
    assert_eq!(game.health(target), num(950));
    let timers = game.world.resource::<Timers>();
    assert!(timers.due(Tick::new(u64::MAX)).is_none());
}

impl Game {
    /// Runs `probe(ctx, unit)` of `source` as a call of `role`, acting as `actor` but in the
    /// mode's, then applies its effects: what it returned, or why it failed.
    fn probe(
        &mut self,
        source: &str,
        role: ScriptRole,
        actor: StableId,
        unit: StableId,
    ) -> Result<Dynamic, CallError> {
        let ctx = self.world.non_send::<Ctx>().clone();
        ctx.view().read(&self.world);
        match role {
            ScriptRole::Mode => ctx.frame().begin_mode(&self.world, false),
            ScriptRole::Ai => ctx.frame().begin_think(&self.world, actor),
            ScriptRole::Action => ctx
                .frame()
                .begin_cast(&self.world, self.strike, 1, actor)
                .unwrap(),
            ScriptRole::Modifier => {
                let blessing = Stats::modifier(&self.world, 0, "blessing").unwrap();
                ctx.frame()
                    .begin_hook(&self.world, blessing, None, 1, Some(actor), 1)
                    .unwrap();
            }
        }
        let handle = ctx.view().unit(unit).unwrap();
        let returned = {
            let mut host = self.world.non_send_mut::<ScriptHost>();
            let script = host.compile(source).unwrap();
            let mut budget = Budget::new(u64::MAX);
            host.call(&mut budget, script, "probe", (ctx.clone(), handle))
        };
        let returned = returned.map_err(CallError::from_script)?;
        let now = self.world.resource::<SimTick>().start();
        ctx.apply(&mut self.world, now);
        Ok(returned)
    }
}

#[test]
fn every_role_reads_the_match_and_deals_damage_heals_and_restores() {
    let probe = r#"
fn probe(ctx, unit) {
    ctx.damage(unit, 10, "true");
    ctx.heal(unit, 4);
    ctx.restore(unit, 3);
    ctx.add_resource(0, "gold", 5);
    [ctx.teams, ctx.map.paths, ctx.avatars().len(), ctx.units_tagged("avatar").len()]
}
"#;
    let mut game = Game::new(SCRIPT, LIMITS);
    game.tick(&[(0, input("hero", "hero-x"))]);
    let actor = game.fighter(0, &[]);
    let target = game.fighter(1, &[]);
    let entity = game.world.resource::<EntityIndex>().get(target).unwrap();
    game.world.get_mut::<Health>(entity).unwrap().take(num(20));
    let mut pool = ResourcePool::new(num(100)).unwrap();
    pool.spend(num(50));
    game.world.entity_mut(entity).insert(pool);
    // Each role in turn: 4 healed, 3 restored and 5 gold given as its effects apply, then 10
    // dealt in the tick's damage pass: from 980, 984 then 974, and so on; the pool from 50, 3 a
    // call.
    let gold = |game: &Game| {
        let resources = game.world.resource::<PlayerResources>();
        resources.amount(PlayerSlot::new(0), "gold")
    };
    for (at, role) in ScriptRole::ALL.into_iter().enumerate() {
        let at = i64::try_from(at).unwrap();
        let before = gold(&game);
        let read = game.probe(probe, role, actor, target).unwrap();
        assert_eq!(gold(&game), before + 5, "{role:?}");
        let read: Array = read.cast();
        let names = |value: &Dynamic| -> Vec<String> {
            let list: Array = value.clone().cast();
            list.into_iter().map(|name| name.to_string()).collect()
        };
        assert_eq!(names(&read[0]), ["a", "b"], "{role:?}");
        assert_eq!(names(&read[1]), ["mid"], "{role:?}");
        assert_eq!(
            (read[2].as_int(), read[3].as_int()),
            (Ok(1), Ok(1)),
            "{role:?}"
        );
        assert_eq!(game.health(target), num(980 - 6 * at + 4), "{role:?}");
        game.tick(&[]);
        assert_eq!(game.health(target), num(980 - 6 * (at + 1)), "{role:?}");
        let pool = game.world.get::<ResourcePool>(entity).unwrap().current();
        assert_eq!(pool, num(50 + 3 * (at + 1)), "{role:?}");
    }
    // A call given to other roles fails in this one, when it runs.
    let refused = [
        ("ctx.timer(\"late\", 100, false, ())", ScriptRole::Action),
        ("ctx.end(())", ScriptRole::Ai),
        ("ctx.state.phase", ScriptRole::Modifier),
        ("ctx.order_follow_path(unit)", ScriptRole::Mode),
    ];
    for (call, role) in refused {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = game.probe(&source, role, actor, actor).unwrap_err();
        assert!(
            matches!(failed, CallError::Api(ApiError::NotForRole)),
            "{call} in {role:?}: {failed}"
        );
    }
}
