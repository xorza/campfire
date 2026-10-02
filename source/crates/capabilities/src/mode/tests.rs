use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};
use std::slice;

use bevy_ecs::query::With;
use campfire_content::PackagePath;
use campfire_math::{Num, Ticks, Vec3};
use campfire_script::{Budget, ScriptHost, ScriptId};
use campfire_sim::{
    Capability, Position, SimComponent, SimResource, SimUpdate, StableId, TickInput,
};

use super::*;
use crate::actions::Actions;
use crate::actions::action_book::ActionBook;
use crate::actions::action_data::{ActionData, Targeting};
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionTarget;
use crate::actions::slot_kind::SlotKind;
use crate::actions::slot_kinds::{SlotKindData, SlotKinds};
use crate::capability_set::internals::TestMatch;
use crate::combat::combat_rules::CombatRules;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::heal::{Heal, HealCause};
use crate::combat::on_death::OnDeath;
use crate::combat::pass_queue::PassQueue;
use crate::combat::recent_attackers::RecentAttackers;
use crate::mode::choice_data::{ChoiceData, Offers};
use crate::mode::map_data::{GridData, MarkerData, PathData, PlacedUnitData};
use crate::mode::map_data::{MapData, MapPoint};
use crate::mode::match_end::MatchResult;
use crate::mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
use crate::mode::mode_setup::{LoadoutSetup, SlotAction, UnitTypeSetup};
use crate::mode::mode_state_decl::{ModeStateDecl, SyncTo};
use crate::mode::offer::Offer;
use crate::mode::relation_data::RelationData;
use crate::mode::team_manifest::TeamManifest;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::destination::Destination;
use crate::navigation::navigation_rules::NavigationRules;
use crate::navigation::path_walker::PathEnd;
use crate::navigation::paths::Paths;
use crate::navigation::route::Route;
use crate::navigation::walker::Walker;
use crate::players::resource_id::ResourceId;
use crate::production::production_data::ProductionData;
use crate::production::train_queue::{Queued, TrainQueue};
use crate::progression::Progression;
use crate::progression::experience::Experience;
use crate::progression::track_data::{Thresholds, TrackData};
use crate::progression::track_set::TrackSet;
use crate::scripts::error::ApiError;
use crate::scripts::hook::ScriptRole;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::state_decl::{StateDecl, StateDefault, StateType};
use crate::scripts::state_value::StateValue;
use crate::stats;
use crate::stats::Stats;
use crate::stats::level::Level;
use crate::stats::lifetime::{Ends, Hold, Lifetime};
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifiers::Modifiers;
use crate::stats::modifiers::{Application, Instance, StatShare};
use crate::stats::move_step::MoveStep;
use crate::stats::player_modifiers::{PlayerModifier, PlayerModifiers};
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::unit_stats::UnitStats;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::layer::Layer;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::tag_set::TagSet;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::attitude::Attitude;
use crate::values::bounds::Bounds;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::grid::Grid;
use crate::values::metric::Metric;
use crate::values::number::Number;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;
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
/// The pools the scripts name, the life pool first, as it is until a mode binds one. The mode's
/// data declares none, so no stat sets their maxima.
const POOLS: [&str; 2] = ["health", "mana"];
/// The players' resources the mode declares.
const RESOURCES: [&str; 2] = ["gold", "gems"];
const MANA: PoolId = PoolId::new(1).unwrap();

const STATS_3V3: [&str; 9] = [
    "armor",
    "armor_pen",
    "armor_pen_pct",
    "crit_chance",
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
    for camp in ctx.map.markers("camp") {
        ctx.spawn_unit(camp.params.unit_type, "neutral", camp.pos);
    }
    ctx.spawn_group("a", "mid", "start", ctx.p.group);
    ctx.spawn_group("b", "mid", "end", ["grunt"]);
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
        pick(ctx, player, value);
    } else if name == "spells" {
        ctx.choose(player, "spells", value);
    } else if name == "rich" {
        ctx.add_resource(player, value, 9223372036854775807);
    } else if name == "gold" {
        ctx.add_resource(player, value, ctx.p.gold);
        ctx.add_resource(player, value, ctx.p.gold);
    } else if name == "fail" {
        ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].pos);
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

/// Every test script's way to pick: player `player` chooses the hero `hero`, which spawns at the
/// spawn marker of its team, with the spells the player chose.
const PICK: &str = r#"
fn pick(ctx, player, hero) {
    ctx.choose(player, "hero", hero);
    let team = ctx.team_of(player);
    for marker in ctx.map.markers("spawn") {
        if marker.team == team {
            let unit = ctx.spawn_unit(hero, team, marker.pos, player);
            ctx.grant(unit, "spell", ctx.chosen(player, "spells"));
        }
    }
}
"#;

/// The reference 3v3's `on_unit_died`, `hero_died` and `share_xp` as they were when these tests
/// were written.
const DEATHS_3V3: &str = r#"
fn on_unit_died(ctx, unit, killer, assisters) {
    if unit.has_tag("core") {
        ctx.end(ctx.enemy_team(unit.team));
        return;
    }
    share_xp(ctx, unit);
    if unit.is_avatar {
        hero_died(ctx, unit, killer, assisters);
        return;
    }
    if killer != () && killer.is_avatar {
        ctx.add_resource(killer.owner, "gold", unit.params.gold);
    }
    if unit.has_tag("inhibitor") {
        ctx.respawn(unit, ctx.p.inhibitor_respawn_ms);
    } else if unit.has_tag("objective") {
        if killer != () {
            for hero in ctx.avatars(killer.team) {
                ctx.add_modifier(hero, "warden_blessing");
            }
        }
        ctx.respawn(unit, ctx.p.warden_respawn_ms);
    } else if unit.has_tag("camp") {
        ctx.respawn(unit, ctx.p.camp_respawn_ms);
    }
}

fn hero_died(ctx, hero, killer, assisters) {
    ctx.respawn(hero, ctx.p.respawn_base_ms + ctx.p.respawn_per_level_ms * hero.level);
    if killer == () || !killer.is_avatar {
        return;
    }
    let gold = ctx.p.kill_gold;
    if !ctx.state.first_blood {
        gold += ctx.p.first_blood_gold;
        ctx.state.first_blood = true;
    }
    ctx.add_resource(killer.owner, "gold", gold);
    for unit in assisters {
        ctx.add_resource(unit.owner, "gold", ctx.p.assist_gold / assisters.len());
    }
}

fn share_xp(ctx, unit) {
    let heroes = ctx.find(unit, unit.pos, ctx.p.xp_radius, "enemies:avatar");
    if heroes.is_empty() {
        return;
    }
    let xp = if unit.is_avatar {
        ctx.p.hero_xp_base + ctx.p.hero_xp_per_level * unit.level
    } else {
        unit.params.xp
    };
    for hero in heroes {
        ctx.add_xp(hero, "level", num(xp) / heroes.len());
    }
}
"#;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn point(x: i64, z: i64) -> MapPoint {
    MapPoint::Ground([Scalar::Int(x), Scalar::Int(z)])
}

/// A marker `name` with `tag` at (`x`, `z`), of `team` if it names one, with `params`.
fn marker(
    name: &str,
    tag: &str,
    (x, z): (i64, i64),
    team: Option<&str>,
    params: &[(&str, ModeParam)],
) -> MarkerData {
    MarkerData {
        name: DeclaredName::new(name).unwrap(),
        tags: vec![DeclaredName::new(tag).unwrap()],
        pos: Some(point(x, z)),
        region: None,
        team: team.map(|name| DeclaredName::new(name).unwrap()),
        params: params
            .iter()
            .map(|(name, param)| (DeclaredName::new(name).unwrap(), param.clone()))
            .collect(),
        events: false,
    }
}

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
}

fn field(kind: StateType, default: Option<StateDefault>) -> StateDecl {
    StateDecl::new(kind, default).unwrap()
}

/// The unit kit of a grunt: 10 health, combat that keeps it when it dies, and a step of 1 m.
fn grunt() -> UnitKit {
    UnitKit {
        pools: Some(Pools::life(num(10))),
        on_death: Some(OnDeath::Stay),
        step: Some(MoveStep::new(Num::ONE).unwrap()),
        sight: None,
        body: None,
        tracks: TrackSet::default(),
        queue: None,
    }
}

/// The test mode's tracks: `level`, its units' level, of levels 2 at 100 and 3 at 300; and
/// `valor`, of level 2 at 50.
fn tracks() -> BTreeMap<DeclaredName, TrackData> {
    let track = |levels: &[i64], level| TrackData {
        levels: Thresholds::new(levels.iter().map(|&value| num(value))).unwrap(),
        level,
    };
    [
        ("level", track(&[100, 300], true)),
        ("valor", track(&[50], false)),
    ]
    .map(|(name, track)| (DeclaredName::new(name).unwrap(), track))
    .into()
}

/// A cast of no target, cost, time or script: the test mode's spell and hero X's ability.
fn blink_data() -> ActionData {
    ActionData {
        kind: ActionKind::Cast,
        script: None,
        targeting: Targeting::None,
        range: None,
        cooldown_ms: None,
        cost: BTreeMap::new(),
        windup_ms: None,
        clamp_to_range: false,
        toggle: None,
        channel: None,
        hold: None,
        charges: None,
        charge: None,
        passive_modifier: None,
        passive_while_ready: false,
        delivery: None,
        rate: None,
        damage: None,
        damage_kind: None,
        params: BTreeMap::new(),
        projectile_state: BTreeMap::new(),
        on_resolve: Vec::new(),
        on_hit: Vec::new(),
        on_end: Vec::new(),
        unit_type: None,
    }
}

/// Bounds from (−10, −5) to (10, 6) with a grid of 1 m cells; one path, `mid`, along x; team a's
/// spawn at z = −5 and b's at 5; a's tower 8 m down the path; and a camp of a grunt in the middle.
fn map() -> MapData {
    let grunt = ("unit_type", ModeParam::Text("grunt".to_owned()));
    MapData {
        metric: Metric::Planar,
        bounds: Bounds::new([num(-10), num(-5)], [num(10), num(6)]).unwrap(),
        grid: Some(GridData {
            cell: Scalar::Int(1),
        }),
        navigation: Some(GridData {
            cell: Scalar::Int(1),
        }),
        paths: vec![PathData {
            name: DeclaredName::new("mid").unwrap(),
            points: vec![point(-10, 0), point(0, 0), point(10, 0)],
        }],
        units: vec![PlacedUnitData {
            unit_type: DeclaredName::new("tower").unwrap(),
            team: DeclaredName::new("a").unwrap(),
            pos: point(-8, 0),
            path: Some(DeclaredName::new("mid").unwrap()),
            from: None,
        }],
        markers: vec![
            marker("a_spawn", "spawn", (0, -5), Some("a"), &[]),
            marker("b_spawn", "spawn", (0, 5), Some("b"), &[]),
            marker("camp", "camp", (0, 0), None, slice::from_ref(&grunt)),
        ],
    }
}

/// `map` made spatial, each point up at `y`.
fn raised(mut map: MapData, y: i64) -> MapData {
    let raise = |point: &mut MapPoint| {
        let MapPoint::Ground([x, z]) = *point else {
            panic!("the test map's points are on the ground");
        };
        *point = MapPoint::Space([x, Scalar::Int(y), z]);
    };
    map.metric = Metric::Spatial;
    for path in &mut map.paths {
        path.points.iter_mut().for_each(raise);
    }
    for unit in &mut map.units {
        raise(&mut unit.pos);
    }
    for marker in &mut map.markers {
        marker.pos.iter_mut().for_each(raise);
    }
    map
}

/// The test mode's own files: its data, its map and its teams.
#[derive(Debug)]
struct ModeFiles {
    data: ModeData,
    /// The mode's modifiers, which its package's content holds.
    modifiers: BTreeMap<String, ModifierData>,
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
        affects: None,
        params: BTreeMap::new(),
        state: [(
            DeclaredName::new("count").unwrap(),
            field(StateType::Int, None),
        )]
        .into(),
    }
}

/// An upgrade a player holds for its grunts, with no end.
fn drill() -> ModifierData {
    ModifierData {
        duration_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        affects: FilterData::parse("allies:grunt"),
        state: BTreeMap::new(),
        ..blessing()
    }
}

/// The test mode's slot kinds: `basic`, of 2 ranks, and `spell`, learned from the spawn.
fn slot_kinds() -> SlotKinds {
    let kind = |name, ranks| SlotKindData {
        name: DeclaredName::new(name).unwrap(),
        ranks: NonZeroU8::new(ranks),
        levels: Vec::new(),
    };
    SlotKinds(vec![kind("basic", 2), kind("spell", 0)])
}

/// The test mode's choices: a unique hero, one spell, and `duo`, two avatars any player may
/// share.
fn choices() -> BTreeMap<DeclaredName, ChoiceData> {
    [
        ("hero", Offers::Avatars, true, 1, None),
        ("spells", Offers::Loadout, false, 1, Some("spell")),
        ("duo", Offers::Avatars, false, 2, None),
    ]
    .map(|(name, offers, unique, count, slot)| {
        let choice = ChoiceData {
            offers,
            unique,
            count: NonZeroU8::new(count).unwrap(),
            slot: slot.map(|kind| DeclaredName::new(kind).unwrap()),
        };
        (DeclaredName::new(name).unwrap(), choice)
    })
    .into()
}

/// The resource `name` of the mode's `RESOURCES`.
fn resource(name: &str) -> Option<ResourceId> {
    let names = RESOURCES.map(|name| DeclaredName::new(name).unwrap());
    ResourceId::named(&names, name)
}

/// The mode's state fields, each sent to all.
fn mode_state() -> BTreeMap<DeclaredName, ModeStateDecl> {
    [
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
    .map(|(name, decl)| {
        let sync = SyncTo::All;
        (
            DeclaredName::new(name).unwrap(),
            ModeStateDecl { decl, sync },
        )
    })
    .into()
}

fn mode_files() -> ModeFiles {
    let text = |text: &str| ListEntry::Text(text.to_owned());
    ModeFiles {
        data: ModeData {
            script: PackagePath::parse("scripts/mode.rhai").unwrap(),
            combat: CombatRules {
                damage_kinds: DAMAGE_KINDS
                    .map(|kind| DeclaredName::new(kind).unwrap())
                    .into(),
                ..CombatRules::default()
            },
            navigation: NavigationRules {
                layers: ["ground", "air"]
                    .map(|name| DeclaredName::new(name).unwrap())
                    .into(),
            },
            slots: slot_kinds(),
            choices: choices(),
            inputs: [
                ("hero", InputType::String),
                ("spells", InputType::StringList),
                ("rich", InputType::String),
                ("gold", InputType::String),
                ("fail", InputType::String),
                ("phase", InputType::String),
                ("probe", InputType::String),
            ]
            .map(|(name, kind)| (DeclaredName::new(name).unwrap(), kind))
            .into(),
            state_version: None,
            state: mode_state(),
            params: [
                ("group", ModeParam::List(vec![text("grunt"), text("grunt")])),
                ("gold", ModeParam::Value(Scalar::Int(8))),
                ("xp_radius", ModeParam::Value(Scalar::Int(16))),
                ("hero_xp_base", ModeParam::Value(Scalar::Int(150))),
                ("hero_xp_per_level", ModeParam::Value(Scalar::Int(25))),
                ("respawn_base_ms", ModeParam::Value(Scalar::Int(1000))),
                ("respawn_per_level_ms", ModeParam::Value(Scalar::Int(500))),
            ]
            .map(|(name, param)| (DeclaredName::new(name).unwrap(), param))
            .into(),
            stats: STATS_3V3
                .map(|name| (Stat::named(name).unwrap(), StatRule::default()))
                .into(),
            pools: BTreeMap::new(),
            resources: RESOURCES
                .map(|name| DeclaredName::new(name).unwrap())
                .into(),
            relations: vec![RelationData {
                teams: ["a", "neutral"].map(|name| DeclaredName::new(name).unwrap()),
                relation: Attitude::Neutral,
                vision: true,
            }],
            tags: BTreeMap::new(),
            tracks: tracks(),
        },
        modifiers: [
            ("blessing".to_owned(), blessing()),
            ("drill".to_owned(), drill()),
        ]
        .into(),
        map: map(),
        teams: vec![
            TeamManifest {
                name: DeclaredName::new("a").unwrap(),
                slots: 2,
            },
            TeamManifest {
                name: DeclaredName::new("b").unwrap(),
                slots: 1,
            },
            TeamManifest {
                name: DeclaredName::new("neutral").unwrap(),
                slots: 0,
            },
        ],
    }
}

/// The test mode's setup from `files`, of `script`: its grunt, tower and two heroes' unit types,
/// the heroes on both tracks, its one spell, and hero X's one ability, `strike`.
fn setup(
    files: &ModeFiles,
    script: ScriptId,
    types: [UnitType; 4],
    spell: LoadoutSetup,
    strike: ActionId,
    blessing: ModifierId,
) -> ModeSetup<'_> {
    let [grunt_type, tower_type, x, y] = types;
    let hero = grunt().with_tracks(TrackSet::of([0, 1].map(|at| TrackId::new(at).unwrap())));
    let unit = |unit_type, kit| UnitTypeSetup {
        unit_type,
        kit,
        actions: Vec::new(),
        passive: None,
    };
    ModeSetup {
        script,
        data: &files.data,
        teams: &files.teams,
        players: 3,
        unit_types: vec![
            unit(grunt_type, grunt()),
            UnitTypeSetup {
                actions: vec![SlotAction {
                    kind: SlotKind::new(0),
                    ability: strike,
                }],
                ..unit(x, hero)
            },
            UnitTypeSetup {
                passive: Some(blessing),
                ..unit(y, hero)
            },
            unit(
                tower_type,
                UnitKit {
                    step: None,
                    body: Body::new(num(1)).map(|body| body.on(Layer::new(1))),
                    ..grunt()
                },
            ),
        ],
        avatars: vec!["hero-x".to_owned(), "hero-y".to_owned()],
        loadout: vec![spell],
        walkers: vec![Walker::of(grunt().body.as_ref())],
    }
}

#[derive(Debug)]
struct Game {
    world: World,
    /// The one spell's ability.
    blink: ActionId,
    /// Hero X's ability, of 2 ranks.
    strike: ActionId,
}

impl Game {
    /// A match of the test mode for 3 players, two on team `a` and one on `b`, with `limits`.
    fn new(script: &str, limits: ScriptLimits) -> Game {
        Game::start(script, limits, mode_files()).unwrap()
    }

    /// The match `new` gives, of the mode `files`; an error when the mode's start fails.
    fn start(script: &str, limits: ScriptLimits, files: ModeFiles) -> Result<Game, CallError> {
        let scripts = ScriptBudgets::new(limits, 3);
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Abilities,
            Capability::Progression,
            Capability::Production,
        ];
        let TestMatch {
            mut world,
            mut schedule,
            mut registry,
        } = TestMatch::new(&declared, RATE, Some(scripts));
        let resources: Vec<&str> = files
            .data
            .resources
            .iter()
            .map(DeclaredName::as_str)
            .collect();
        Units::name_kinds(&world, &DAMAGE_KINDS, &POOLS, &resources);
        let layers = files.data.navigation.layers.iter();
        Units::declare_tags(&mut world, layers.map(DeclaredName::as_str));
        let mut load = |name: &str, tag: &str| {
            let data = UnitTypeData {
                tags: vec![DeclaredName::new(tag).unwrap()],
                params: BTreeMap::new(),
            };
            Units::load_type(&mut world, TypeScope::Mode, name, &data)
        };
        let (grunt_type, tower_type) = (load("grunt", "grunt"), load("tower", "tower"));
        let (x, y) = (load("hero-x", "avatar"), load("hero-y", "avatar"));
        // A type outside the mode's kits, as a projectile's is: the view knows it, a spawn does not.
        load("bolt", "projectile");
        // The stats first, as a match's books know them before any action or modifier; the mode's
        // books give the full book at install.
        stats::loads::load_stats(&mut world, &files.data.stats);
        let blink = blink_data();
        // A spell has one rank; hero X's ability, 2.
        let strike = Actions::load(&mut world, 0, "strike", &blink, None, 2).unwrap();
        let blink = Actions::load(&mut world, 0, "blink", &blink, None, 1).unwrap();
        let spell = LoadoutSetup {
            id: "blink".to_owned(),
            ability: blink,
        };
        let types = [grunt_type, tower_type, x, y];
        Progression::load(&mut world, &files.data.tracks);
        for (name, data) in &files.modifiers {
            Stats::load_modifier(&mut world, 0, name, data, None);
        }
        let script = Units::compile(&mut world, &format!("{script}{PICK}")).unwrap();
        let blessing = Stats::modifier(&world, 0, "blessing").unwrap();
        let setup = setup(&files, script, types, spell, strike, blessing);
        let books = {
            let view = world.non_send::<View>();
            let stats = StatBook::new(&files.data.stats, [], num(10));
            let relations = &files.data.relations;
            let unit_type = |name: &str| view.unit_type_named(name);
            let map = ModeMap::resolve(&files.map, &files.teams, relations, unit_type).unwrap();
            let unit_types = &setup.unit_types;
            ModeBooks::build(&files.data, unit_types, &mut view.types_mut(), stats, map)
        };
        Mode::install(&mut world, &mut schedule, &mut registry, setup, books);
        // The scripts name pools the data does not declare, whose maxima no stat sets.
        Units::name_kinds(&world, &DAMAGE_KINDS, &POOLS, &RESOURCES);
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
        let at = ModeBook::of(ctx)
            .unwrap()
            .schema
            .state_field_named(name)
            .unwrap()
            .index;
        self.world.resource::<ModeState>().get()[at].clone()
    }

    /// Each unit: its id, where it stands, its team, and the end of a path it walks from.
    fn units(&self) -> Vec<(u64, Position, u8, Option<PathEnd>)> {
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
                    unit.get::<PathWalker>().map(|walker| walker.walks_from()),
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
    // Before tick 0: the map's tower, 0, then the match start's spawns in order: the camp's
    // grunt, 1, at its marker; team a's spawn group of two, 2 and 3, at the path's start; team
    // b's spawn group of one, 4, at its end. Teams a and b are 0 and 1, neutral 2.
    assert_eq!(
        game.units(),
        [
            (0, at(-8, 0), 0, None),
            (1, at(0, 0), 2, None),
            (2, at(-10, 0), 0, Some(PathEnd::Start)),
            (3, at(-10, 0), 0, Some(PathEnd::Start)),
            (4, at(10, 0), 1, Some(PathEnd::End)),
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
    // Each unit type has the tag of the layer it moves on: the tower, of the second layer, `air`;
    // the grunt, with no body, the first's, `ground`.
    let tags = |id| game.world.get::<UnitTags>(game.entity(id)).unwrap().tags;
    let tag = |name| {
        game.world
            .non_send::<View>()
            .types_mut()
            .tag_named(name)
            .unwrap()
    };
    let layer_tags = [0, 1].map(|id| [tag("ground"), tag("air")].map(|tag| tags(id).contains(tag)));
    assert_eq!(layer_tags, [[false, true], [true, false]]);
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
    // and player 2's choice of the other hero. The thrown call fails with no refusal of the API,
    // and the id it took for its grunt is free again.
    assert_eq!(game.field("inputs"), StateValue::Int(3));
    assert_eq!(game.field("phase"), StateValue::Text("start".to_owned()));
    assert_eq!(
        game.failures(),
        [
            Some(ApiError::ChoiceTaken),
            Some(ApiError::ChoiceCount),
            None,
            Some(ApiError::WrongStateType)
        ]
    );
    // Each player's row: duo's two values, hero's, spells', the choices by name. Player 0 chose
    // hero X, offer 0, and blink, the one spell; player 2 hero Y, offer 1.
    let offer = |index| Some(Offer::new(index));
    let chosen = &game.world.resource::<Choices>().0;
    let rows: Vec<_> = chosen.chunks(4).collect();
    assert_eq!(
        rows,
        [
            [None, None, offer(0), offer(0)],
            [None; 4],
            [None, None, offer(1), None],
        ]
    );
    let next = game.world.resource_mut::<IdAllocator>().allocate();
    assert_eq!(next.get(), 7);
    // The heroes, 5 and 6, at their teams' spawns under their players' control: player 0's
    // with its own ability unlearned, then its spell learned.
    let heroes: Vec<_> = game.units()[5..].to_vec();
    assert_eq!(heroes, [(5, at(0, -5), 0, None), (6, at(0, 5), 1, None)]);
    let hero = game.entity(5);
    assert_eq!(
        game.world.get::<Owner>(hero).unwrap().slot(),
        PlayerSlot::new(0)
    );
    let slots = game.world.get::<ActionSlots>(hero).unwrap();
    let slots: Vec<_> = slots.iter().map(|slot| (slot.action, slot.rank)).collect();
    assert_eq!(slots, [(game.strike, 0), (game.blink, 1)]);
}

#[test]
fn a_mode_learns_a_hero_ability_up_to_its_last_rank_and_a_failed_call_learns_nothing() {
    // Hero X holds its ability in slot 0, of 2 ranks, and the spell in slot 1, of 1.
    let learner = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "spells" {
        ctx.choose(player, "spells", value);
        return;
    }
    if name == "hero" {
        pick(ctx, player, value);
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
        let slots = game.world.get::<ActionSlots>(hero).unwrap();
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
fn experience_raises_levels_and_each_level_reached_runs_on_level_up_in_the_tick() {
    let leveler = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
        return;
    }
    let hero = ctx.avatars()[0];
    if value == "99" {
        ctx.add_xp(hero, "level", 99);
    } else if value == "1.5" {
        ctx.add_xp(hero, "level", num(3) / 2);
    } else if value == "500" {
        ctx.add_xp(hero, "level", 500);
    } else if value == "tower" {
        ctx.add_xp(ctx.units_tagged("tower")[0], "level", 1);
    } else if value == "fame" {
        ctx.add_xp(hero, "fame", 1);
    } else if value == "negative" {
        ctx.add_xp(hero, "level", -1);
    }
}

fn on_level_up(ctx, unit, track, level) {
    ctx.state.kind += `${track} ${level};`;
    if track == "level" && level == 3 {
        ctx.add_xp(unit, "valor", 50);
    }
}
"#;
    let mut game = Game::new(leveler, LIMITS);
    game.tick(&[(0, input("hero", "hero-x"))]);
    let mut owned = game.world.query_filtered::<Entity, With<Owner>>();
    let hero = owned.single(&game.world).unwrap();
    // The `level` track's level is the unit's own; valor keeps its own.
    let progress = |game: &Game| {
        let experience = game.world.get::<Experience>(hero).unwrap();
        let [level, valor] = [0, 1].map(|at| experience.get(TrackId::new(at).unwrap()).unwrap());
        assert_eq!(level.level, None);
        let unit_level = game.world.get::<Level>(hero).unwrap().get();
        (level.xp, unit_level, valor.xp, valor.level.map(Level::get))
    };

    let half = Num::from_bits(1 << 23);
    // 99 stays below level 2's 100. 1.5 more makes 100.5: level 2, and the unit's level with it.
    // 500 more makes 600.5, past level 3's 300, the last; level 3 adds 50 valor, valor's level 2,
    // and its `on_level_up` runs in the same tick.
    let steps = [
        ("99", (num(99), 1, Num::ZERO, Some(1)), ""),
        ("1.5", (num(100) + half, 2, Num::ZERO, Some(1)), "level 2;"),
        (
            "500",
            (num(600) + half, 3, num(50), Some(2)),
            "level 2;level 3;valor 2;",
        ),
    ];
    for (value, expected, reached) in steps {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(progress(&game), expected, "{value}");
        assert_eq!(
            game.field("kind"),
            StateValue::Text(reached.into()),
            "{value}"
        );
        assert_eq!(game.failures(), [], "{value}");
    }
    // A unit without the track, a track the mode does not declare, and a negative amount fail
    // the call, and change nothing.
    let refused = [
        ("tower", ApiError::NoTrack),
        ("fame", ApiError::UnknownTrack),
        ("negative", ApiError::NegativeXp),
    ];
    for (value, error) in refused {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(game.failures(), [Some(error)], "{value}");
        assert_eq!(
            progress(&game),
            (num(600) + half, 3, num(50), Some(2)),
            "{value}"
        );
    }
}

#[test]
fn a_hero_dead_beside_an_enemy_hero_gives_it_experience_and_comes_back() {
    // The 3v3's `on_unit_died` as it is: hero Y, 10 m from hero X, dies to the mode's damage,
    // with no killer. X takes all of Y's 150 + 25 × 1 experience, past level 2's 100, and Y
    // comes back after 1000 + 500 × 1 ms, as the call that gave the experience did not fail.
    let killer = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
        return;
    }
    ctx.damage(ctx.avatars("b")[0], 1000, "physical");
}
"#;
    let script = format!("{killer}{DEATHS_3V3}");
    let mut game = Game::new(&script, LIMITS);
    // The 3v3's tag of its cores, which `on_unit_died` reads first.
    let core = UnitTypeData {
        tags: vec![DeclaredName::new("core").unwrap()],
        params: BTreeMap::new(),
    };
    Units::load_type(&mut game.world, TypeScope::Mode, "core", &core);
    game.tick(&[(0, input("hero", "hero-x")), (2, input("hero", "hero-y"))]);
    let hero = |game: &mut Game, slot| {
        let mut owned = game.world.query::<(Entity, &Owner)>();
        let (entity, _) = owned
            .iter(&game.world)
            .find(|(_, owner)| owner.slot() == PlayerSlot::new(slot))
            .unwrap();
        entity
    };
    let (x, y) = (hero(&mut game, 0), hero(&mut game, 2));
    game.tick(&[(0, input("probe", "kill"))]);
    assert_eq!(game.failures(), []);
    assert!(game.world.get::<Dead>(y).is_some());
    let xp = |game: &Game| {
        let experience = game.world.get::<Experience>(x).unwrap();
        experience.get(TrackId::new(0).unwrap()).unwrap().xp
    };
    assert_eq!(xp(&game), num(175));
    assert_eq!(game.world.get::<Level>(x).unwrap().get(), 2);
    // Y died in tick 1, and the mode set its respawn from the end of tick 1, the start of tick
    // 2: 1500 ms is 15 ticks at 10 a second, so Y is dead through tick 16 and back in tick 17.
    assert_eq!(
        game.world.get::<Respawn>(y),
        Some(&Respawn { at: Tick::new(17) })
    );
    for _ in 2..=16 {
        game.tick(&[]);
        assert!(game.world.get::<Dead>(y).is_some());
    }
    game.tick(&[]);
    assert!(game.world.get::<Dead>(y).is_none());
}

#[test]
fn a_train_pays_at_once_joins_the_queue_and_spawns_its_unit_when_its_time_ends() {
    let picker = r"
fn on_mode_input(ctx, player, name, value) {
    pick(ctx, player, value);
}
";
    let mut game = Game::new(picker, LIMITS);
    game.tick(&[(0, input("hero", "hero-x"))]);
    // A grunt for 30 mana and 5 gold, in 300 ms, 3 ticks at 10 a second.
    let int = |value| Ranked::One(Number::Value(Scalar::Int(value)));
    let cost = ["mana", "gold"].map(|name| DeclaredName::new(name).unwrap());
    let train = ActionData {
        kind: ActionKind::Train,
        cost: [(cost[0].clone(), int(30)), (cost[1].clone(), int(5))].into(),
        windup_ms: Some(int(300)),
        unit_type: Some(DeclaredName::new("grunt").unwrap()),
        ..blink_data()
    };
    let train = Actions::load(&mut game.world, 0, "train_grunt", &train, None, 1).unwrap();
    // Hero X becomes a producer of a queue of 2, with 100 mana, and its player holds 15 gold.
    let mut owned = game.world.query_filtered::<Entity, With<Owner>>();
    let producer = owned.single(&game.world).unwrap();
    let pools = Pools::new([(PoolId::FIRST, num(10)), (MANA, num(100))]).unwrap();
    let slots = ActionSlots::new([(train, SlotKind::new(0), 1)]);
    let hero_type = *game.world.get::<UnitType>(producer).unwrap();
    let production = ProductionData {
        queue: NonZeroU8::new(2).unwrap(),
    };
    let mut producers = game.world.resource_mut::<ByType<ProductionData>>();
    producers.set(hero_type, production);
    game.world
        .entity_mut(producer)
        .insert((pools, slots, TrainQueue::default()));
    let gold = resource("gold").unwrap();
    let player = PlayerSlot::new(0);
    game.world
        .resource_mut::<PlayerResources>()
        .add(player, gold, 15)
        .unwrap();
    let order = |game: &mut Game| {
        let mut slots = game.world.get_mut::<ActionSlots>(producer).unwrap();
        slots.order(0, ActionTarget::None);
    };
    let paid = |game: &Game| {
        let mana = game
            .world
            .get::<Pools>(producer)
            .unwrap()
            .current(MANA)
            .unwrap();
        let held = game
            .world
            .resource::<PlayerResources>()
            .amount(player, gold);
        (mana.round(), held)
    };
    let mut grunts = game
        .world
        .query_filtered::<(&UnitType, &Owner, &Position), With<Owner>>();
    let hero = *game.world.get::<UnitType>(producer).unwrap();
    let mut trained = |game: &mut Game| {
        let units = grunts.iter(&game.world);
        units.filter(|&(&unit_type, ..)| unit_type != hero).count()
    };

    // Ticks 0 and 1 of the queue each order a train: each pays 30 mana and 5 gold at once. The
    // first's 3 ticks run from time 0 to time 3, the end of tick 2, so its unit spawns in tick 2's
    // Mode stage; the second's run from then to time 6, tick 5. A third, ordered in tick 2's Act
    // stage, before the first spawns, finds the queue full: nothing is paid.
    let steps = [
        (true, (70, 10), 0),
        (true, (40, 5), 0),
        (true, (40, 5), 1),
        (false, (40, 5), 1),
        // Room again: a third pays, and runs from time 6 to time 9, the end of tick 8.
        (true, (10, 0), 1),
        (false, (10, 0), 2),
        (false, (10, 0), 2),
        // Room again, and 10 mana of 30: the order fails, and nothing is paid.
        (true, (10, 0), 2),
        (false, (10, 0), 3),
        (false, (10, 0), 3),
    ];
    for (at, (ordered, expected, made)) in steps.into_iter().enumerate() {
        if ordered {
            order(&mut game);
        }
        game.tick(&[]);
        assert_eq!(paid(&game), expected, "tick {at}");
        assert_eq!(trained(&mut game), made, "tick {at}");
    }
    // Each grunt stands where the producer stood, its player's, and the queue is empty.
    let at = *game.world.get::<Position>(producer).unwrap();
    let made: Vec<_> = grunts
        .iter(&game.world)
        .filter(|&(&unit_type, ..)| unit_type != hero)
        .map(|(_, owner, &pos)| (owner.slot(), pos))
        .collect();
    assert_eq!(made, [(player, at); 3]);
    assert!(
        game.world
            .get::<TrainQueue>(producer)
            .unwrap()
            .entries()
            .is_empty()
    );
    assert_eq!(game.failures(), []);
}

#[test]
fn a_mode_applies_a_modifier_writes_its_handle_and_sees_it_end() {
    let blesser = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
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
                    instance.lifetime.until().map(Tick::get),
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
                instance.lifetime.held_by(Hold::Passive),
                instance.lifetime.until(),
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
        (1, input("gold", "silver")),
        (0, input("probe", "")),
    ]);
    // Gold, the first of the mode's resources, and gems, the second; a resource the mode does
    // not declare fails, as a sum past what an integer holds does.
    let resources = game.world.resource::<PlayerResources>();
    let amount = |slot, name| {
        let resource = resource(name).unwrap();
        resources.amount(PlayerSlot::new(slot), resource)
    };
    assert_eq!(
        [amount(1, "gold"), amount(1, "gems"), amount(0, "gold")],
        [16, i64::MAX, 0]
    );
    assert_eq!(resource("gems").map(ResourceId::index), Some(1));
    assert_eq!(
        game.failures(),
        [
            Some(ApiError::ResourceOverflow),
            Some(ApiError::UnknownResource)
        ]
    );
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
    for unit_type in ["ghost", "bolt"] {
        let failing = format!(
            "fn on_match_start(ctx) {{ ctx.spawn_unit(\"{unit_type}\", \"a\", ctx.map.markers(\"camp\")[0].pos); }}"
        );
        let failed = Game::start(&failing, LIMITS, mode_files()).err();
        assert!(
            matches!(failed, Some(CallError::Api(ApiError::UnknownUnitType))),
            "{unit_type}: {failed:?}"
        );
    }
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
fn a_death_or_a_level_up_whose_call_finds_the_mode_pool_spent_waits_with_its_unit() {
    // Each death call spins 50 additions: a mode pool of 500 operations holds two of them, not
    // three, and then the third and a level-up.
    let script = r#"
fn on_match_start(ctx) {
    ctx.spawn_group("b", "mid", "end", ["grunt", "grunt", "grunt"]);
}

fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
    } else {
        ctx.add_xp(ctx.avatars()[0], "level", 100);
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {
    ctx.state.kind += `${unit.team} by ${killer.team};`;
    let spun = 0;
    for i in 0..50 { spun += i; }
}

fn on_level_up(ctx, unit, track, level) {
    ctx.state.kind += `${track} ${level};`;
}
"#;
    let limits = ScriptLimits {
        mode: 500,
        ..LIMITS
    };
    let mut game = Game::new(script, limits);
    game.tick(&[(0, input("hero", "hero-x"))]);
    // Units: the tower 0 of a, b's grunts 1 to 3, which despawn when they die, and the hero 4.
    // The tower's damage kills the three grunts in tick 1, as the hero reaches level 2.
    let grunts = [1, 2, 3].map(|at| game.entity(at));
    let tower = game.world.get::<StableId>(game.entity(0)).copied();
    for grunt in grunts {
        game.world.entity_mut(grunt).insert(OnDeath::Despawn);
        let target = *game.world.get::<StableId>(grunt).unwrap();
        game.world.resource_mut::<PassQueue>().push_damage(Damage {
            source: tower,
            target,
            amount: num(10),
            kind: DamageKind::new(0),
            cause: DamageCause::Effect,
            ability: None,
            depth: 0,
            hit: None,
        });
    }
    let waiting = |game: &Game| {
        let deaths = game.world.resource::<UnansweredDeaths>().iter().count();
        (deaths, game.world.resource::<LevelUps>().0.len())
    };
    let stands = |game: &Game, grunt: Entity| {
        let unit = game.world.get_entity(grunt).ok()?;
        Some((unit.contains::<Dead>(), unit.contains::<Kept>()))
    };
    game.tick(&[(0, input("probe", "xp"))]);
    // Two deaths ran; the third waits, and so does the level-up behind it. The first two grunts
    // despawned; the third stays, dead and kept, for its call.
    assert_eq!(
        game.field("kind"),
        StateValue::Text("b by a;b by a;".to_owned())
    );
    assert_eq!(waiting(&game), (1, 1));
    // The waiting death is state: it decodes to itself, and an assister its runs do not cover
    // fails to decode. Its assisters come last, none: their length is the last byte.
    let unanswered = game.world.resource::<UnansweredDeaths>();
    let mut bytes = postcard::to_allocvec(unanswered).unwrap();
    let decoded = postcard::from_bytes::<UnansweredDeaths>(&bytes).ok();
    assert_eq!(decoded.as_ref(), Some(unanswered));
    assert_eq!(bytes.pop(), Some(0));
    bytes.extend([1, 5]);
    assert!(postcard::from_bytes::<UnansweredDeaths>(&bytes).is_err());
    assert_eq!(
        grunts.map(|grunt| stands(&game, grunt)),
        [None, None, Some((true, true))]
    );
    game.tick(&[]);
    // Both run first in the next tick, with the third grunt and its killer as handles; then it
    // despawns.
    let all = "b by a;b by a;b by a;level 2;";
    assert_eq!(game.field("kind"), StateValue::Text(all.to_owned()));
    assert_eq!(waiting(&game), (0, 0));
    assert_eq!(stands(&game, grunts[2]), None);
    assert_eq!(game.failures(), []);
}

#[test]
fn a_death_reaches_the_mode_and_a_respawn_brings_the_unit_back_at_its_spawn() {
    // The mode spawns as the test mode does, records each death, and respawns the dead 250 ms
    // later: 3 ticks at 10 a second. A probe respawns the first unit with the tag it names.
    let script = r#"
fn on_match_start(ctx) {
    for camp in ctx.map.markers("camp") {
        ctx.spawn_unit(camp.params.unit_type, "neutral", camp.pos);
    }
    ctx.spawn_group("a", "mid", "start", ctx.p.group);
    ctx.spawn_group("b", "mid", "end", ["grunt"]);
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
    game.world.resource_mut::<PassQueue>().push_damage(Damage {
        source: Some(four),
        target: one,
        amount: num(10),
        kind: DamageKind::new(0),
        cause: DamageCause::Effect,
        ability: None,
        depth: 0,
        hit: None,
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
    let pools = game.world.get::<Pools>(victim).unwrap();
    assert_eq!(pools.current(PoolId::FIRST), Some(num(10)));

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
    ctx.spawn_group("a", "mid", "start", ["grunt"]);
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
    assert!(game.failures().is_empty());
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
        let changes: Vec<_> = stats
            .iter()
            .map(|&(name, _)| {
                (
                    book.named(&Stat::named(name).unwrap()).unwrap(),
                    StatOp::Add,
                )
            })
            .collect();
        let mut modifiers = self.world.resource_mut::<ModifierBook>();
        let id = modifiers.push_changes(&changes, TagSet::default());
        let shares = stats
            .iter()
            .map(|&(_, value)| StatShare { value, live: None });
        let instance = Instance {
            id,
            source: None,
            ability: None,
            rank: 1,
            aura_radius: None,
            stacks: 1,
            lifetime: Lifetime::new(None, Ends::Never),
            stack_life: None,
            stack_ends: Vec::new(),
            interval: None,
            shield: None,
            stats: shares.collect(),
            state: Vec::new(),
        };
        let mut modifiers = Modifiers::default();
        modifiers.apply(Application {
            instance,
            reapply: Reapply::Refresh,
            max_stacks: None,
        });
        let grunt = self
            .world
            .non_send::<View>()
            .unit_type_named("grunt")
            .unwrap();
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world.spawn((
            id,
            at(0, 0),
            Team::new(team),
            Pools::life(num(1000)),
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
        self.world.resource_mut::<PassQueue>().push_damage(Damage {
            source,
            target,
            amount: num(amount),
            kind: DamageKind::new(u8::try_from(kind).unwrap()),
            cause,
            ability: None,
            depth: 0,
            hit: None,
        });
    }

    fn health(&self, id: StableId) -> Num {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        let pools = self.world.get::<Pools>(entity).unwrap();
        pools.current(PoolId::FIRST).unwrap()
    }
}

#[test]
fn the_3v3s_calc_damage_weighs_each_hit_exactly() {
    let mut game = Game::new(calc_damage_3v3(), LIMITS);
    let half = Num::ONE / 2;
    // The source deals 50% more, crits on a roll below 0.25, ignores half of armor, then 10
    // more.
    let source = game.fighter(
        0,
        &[
            ("damage_dealt_pct", half),
            ("crit_chance", Num::ONE / 4),
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
    let crit = DamageCause::Attack {
        roll: Num::from_bits((1 << Num::FRAC_BITS) / 4 - 1),
    };
    let attack = DamageCause::Attack { roll: Num::ONE / 4 };
    // 100 physical, its roll of 0.25 no crit, 150 dealt: armor 120 × (1 − 0.5) − 10 = 50, 150 ×
    // 100 ÷ 150 = 100, less the block of 5: 95.
    game.damage(Some(source), armored, 100, "physical", attack);
    // 40 magic, a crit on a roll just below 0.25: 40 × 1.5 × 2 = 120, magic resist 60 with no
    // magic pen: 120 × 100 ÷ 160 = 75, no block.
    game.damage(Some(source), armored, 40, "magic", crit);
    // 100 physical, 150 dealt, against armor −100, which pen does not touch: 150 × (2 − 100 ÷
    // 200) = 225.
    game.damage(Some(source), exposed, 100, "physical", attack);
    // 100 true, 150 dealt, whatever the armor.
    game.damage(Some(source), exposed, 100, "true", DamageCause::Effect);
    // An attack of 100 physical from no source, its roll 0: no bonus and no crit chance, 100 ×
    // 1.5 = 150.
    let lowest = DamageCause::Attack { roll: Num::ZERO };
    game.damage(None, exposed, 100, "physical", lowest);
    game.tick(&[]);
    assert!(game.failures().is_empty());
    assert_eq!(game.health(armored), num(1000 - 95 - 75));
    assert_eq!(game.health(exposed), num(1000 - 225 - 150 - 150));
}

#[test]
fn calc_damage_and_calc_heal_are_pure_outside_the_pools_and_a_failure_keeps_the_amount() {
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

fn calc_heal(ctx, h) {
    if h.leech { h.amount } else { h.amount / 2 }
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

    // A heal of 10 that `calc_heal` halves, and a leech heal of 6 it keeps: 950 + 5 + 6, as
    // the target has no heal scale.
    for (amount, cause) in [(10, HealCause::Effect), (6, HealCause::Leech)] {
        game.world.resource_mut::<PassQueue>().push_heal(Heal {
            source: Some(source),
            target,
            amount: num(amount),
            cause,
            ability: None,
            depth: 0,
        });
    }
    game.tick(&[]);
    assert_eq!(game.failures(), []);
    assert_eq!(game.health(target), num(961));
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
                .begin_cast(&self.world, self.strike, 1, actor, 0, None)
                .unwrap(),
            ScriptRole::Modifier => {
                let blessing = Stats::modifier(&self.world, 0, "blessing").unwrap();
                ctx.frame()
                    .begin_hook(&self.world, blessing, None, 1, Some(actor), 0, 1)
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
    ctx.restore(unit, "mana", 3);
    ctx.add_resource(0, "gold", 5);
    [ctx.teams, ctx.map.paths, ctx.avatars().len(), ctx.units_tagged("avatar").len()]
}
"#;
    let mut game = Game::new(SCRIPT, LIMITS);
    game.tick(&[(0, input("hero", "hero-x"))]);
    let actor = game.fighter(0, &[]);
    let target = game.fighter(1, &[]);
    let entity = game.world.resource::<EntityIndex>().get(target).unwrap();
    let mut pools = Pools::new([(PoolId::FIRST, num(1000)), (MANA, num(100))]).unwrap();
    pools.take(PoolId::FIRST, num(20));
    pools.take(MANA, num(50));
    game.world.entity_mut(entity).insert(pools);
    // Each role in turn: 3 restored and 5 gold given as its effects apply, then 10 dealt and 4
    // healed in the tick's pass, in the order queued: from 980 to 974, and so on; the pool from
    // 50, 3 a call.
    let gold = |game: &Game| {
        let resources = game.world.resource::<PlayerResources>();
        let gold = resource("gold").unwrap();
        resources.amount(PlayerSlot::new(0), gold)
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
        assert_eq!(game.health(target), num(980 - 6 * at), "{role:?}");
        game.tick(&[]);
        assert_eq!(game.health(target), num(980 - 6 * (at + 1)), "{role:?}");
        let pool = game.world.get::<Pools>(entity).unwrap().current(MANA);
        assert_eq!(pool, Some(num(50 + 3 * (at + 1))), "{role:?}");
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

#[test]
fn a_script_turns_a_neutral_pair_hostile_and_filters_follow_it() {
    // Team a and the neutral team regard each other neutral, as the mode declares; b is hostile to
    // both. At the origin: a's and b's fighters, and the map's neutral grunt.
    let mut game = Game::new(SCRIPT, LIMITS);
    game.tick(&[]);
    let a = game.fighter(0, &[]);
    game.fighter(1, &[]);
    let counts = r#"
fn probe(ctx, unit) {
    ["hostiles", "neutrals", "enemies"].map(|filter| ctx.find(unit, unit.pos, 1, filter).len())
}
"#;
    let count = |game: &mut Game| -> Vec<INT> {
        let counts: Array = game.probe(counts, ScriptRole::Mode, a, a).unwrap().cast();
        counts
            .into_iter()
            .map(|count| count.as_int().unwrap())
            .collect()
    };
    // From a: b's fighter is hostile, the grunt neutral, and both are enemies a may attack.
    assert_eq!(count(&mut game), [1, 1, 2]);
    // A unit's script, as a modifier's, turns the pair hostile; the grunt is a hostile then.
    let turn = r#"fn probe(ctx, unit) { ctx.set_relation("neutral", "a", "hostile") }"#;
    let turned = game.probe(turn, ScriptRole::Modifier, a, a).unwrap();
    assert!(turned.is_unit());
    assert_eq!(count(&mut game), [2, 0, 2]);
    // An attitude other than the three, or a team's to itself, fails the call.
    for (call, refused) in [
        (
            r#"ctx.set_relation("a", "b", "angry")"#,
            ApiError::UnknownRelation,
        ),
        (
            r#"ctx.set_relation("b", "b", "neutral")"#,
            ApiError::SelfRelation,
        ),
        (
            r#"ctx.set_relation("a", "c", "neutral")"#,
            ApiError::UnknownTeam,
        ),
    ] {
        let source = format!("fn probe(ctx, unit) {{ {call} }}");
        let failed = game.probe(&source, ScriptRole::Mode, a, a).unwrap_err();
        assert!(
            matches!(failed, CallError::Api(api) if api == refused),
            "{call}: {failed}"
        );
    }
}

#[test]
fn three_teams_walk_a_path_each_from_the_end_it_names() {
    // On the test map made spatial, 3 m up: team a and the neutral team walk `mid` from its start,
    // (−10, 3, 0); b from its end, (10, 3, 0).
    let script = r#"
fn on_match_start(ctx) {
    ctx.spawn_group("a", "mid", "start", ["grunt"]);
    ctx.spawn_group("b", "mid", "end", ["grunt"]);
    ctx.spawn_group("neutral", "mid", "start", ["grunt"]);
}

fn on_mode_input(ctx, player, name, value) {
    ctx.spawn_group("a", "mid", "middle", ["grunt"]);
}
"#;
    let mut files = mode_files();
    files.map = raised(files.map, 3);
    let mut game = Game::start(script, LIMITS, files).unwrap();
    assert_eq!(*game.world.resource::<Metric>(), Metric::Spatial);
    let up = |x| Position::new(Vec3::new(num(x), num(3), Num::ZERO)).unwrap();
    let walkers = |game: &Game| <[_; 3]>::try_from(&game.units()[1..]).unwrap();
    assert_eq!(
        walkers(&game),
        [
            (1, up(-10), 0, Some(PathEnd::Start)),
            (2, up(10), 1, Some(PathEnd::End)),
            (3, up(-10), 2, Some(PathEnd::Start)),
        ]
    );
    // Each walks to the last waypoint from its end: a and the neutral team to (10, 3, 0), b to
    // (−10, 3, 0).
    let paths = game.world.resource::<Paths>();
    let last = walkers(&game).map(|unit| paths.waypoint(PathId::new(0), 2, unit.3.unwrap()));
    assert_eq!(last, [Some(up(10)), Some(up(-10)), Some(up(10))]);
    // An end other than `start` and `end` fails.
    game.tick(&[(0, input("phase", "x"))]);
    assert_eq!(game.failures(), [Some(ApiError::UnknownPathEnd)]);
}

#[test]
fn choices_hold_each_players_values_and_grants_fill_a_slot_kind() {
    let script = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
        return;
    }
    if value == "read" {
        let duo = ctx.chosen(player, "duo");
        ctx.state.kind = if duo.is_empty() { "none" } else { duo[0] + "," + duo[1] };
        ctx.state.seen = if ctx.available(player, "hero", "hero-x") { 1 } else { 0 };
        ctx.state.team = ctx.team_of(player);
    } else if value == "duo" {
        ctx.choose(player, "duo", ["hero-y", "hero-x"]);
    } else if value == "swap" {
        ctx.choose(player, "duo", ["hero-x", "hero-y"]);
    } else if value == "short" {
        ctx.choose(player, "duo", ["hero-y"]);
    } else if value == "twice" {
        ctx.choose(player, "duo", ["hero-x", "hero-x"]);
    } else if value == "stranger" {
        ctx.choose(player, "duo", ["hero-x", "hero-z"]);
    } else if value == "nothing" {
        ctx.choose(player, "nothing", "hero-x");
    } else if value == "grant" {
        ctx.grant(ctx.avatars()[0], "spell", ["blink"]);
    } else if value == "grant_basic" {
        ctx.grant(ctx.avatars()[0], "basic", ["blink"]);
    } else if value == "grant_ultimate" {
        ctx.grant(ctx.avatars()[0], "ultimate", ["blink"]);
    } else if value == "grant_stranger" {
        ctx.grant(ctx.avatars()[0], "spell", ["haste"]);
    }
}
"#;
    let mut game = Game::new(script, LIMITS);
    let probe = |value| input("probe", value);
    let read = |game: &Game| ["kind", "seen", "team"].map(|name| game.field(name));
    let text = |text: &str| StateValue::Text(text.to_owned());
    // Before any choice: no duo, hero X free to player 0, who is on team a.
    game.tick(&[(0, probe("read"))]);
    assert_eq!(read(&game), [text("none"), StateValue::Int(1), text("a")]);
    // Player 0 takes hero X, and spawns it with no spells, as it chose none: player 2, of team
    // b, may not take it, as the hero choice is unique.
    game.tick(&[(0, input("hero", "hero-x")), (2, probe("read"))]);
    assert_eq!(read(&game), [text("none"), StateValue::Int(0), text("b")]);
    // Unit 1, after the map's tower.
    let hero = game.entity(1);
    let slots = |game: &Game| {
        let slots = game.world.get::<ActionSlots>(hero).unwrap();
        let slots = slots.iter().map(|slot| (slot.action, slot.kind, slot.rank));
        slots.collect::<Vec<_>>()
    };
    let [basic, spell] = [0, 1].map(SlotKind::new);
    assert_eq!(slots(&game), [(game.strike, basic, 0)]);
    // A choice not unique: players 1 and 2 both take both heroes, in their own order; then
    // player 1 chooses again, which replaces its values. Too few values, one twice, one the
    // choice does not offer and a choice the mode does not declare fail, and change nothing.
    game.tick(&[
        (1, probe("duo")),
        (2, probe("duo")),
        (1, probe("swap")),
        (1, probe("short")),
        (1, probe("twice")),
        (1, probe("stranger")),
        (1, probe("nothing")),
        (1, probe("read")),
    ]);
    let refused = [
        ApiError::ChoiceCount,
        ApiError::RepeatedChoiceValue,
        ApiError::UnknownChoiceValue,
        ApiError::UnknownChoice,
    ];
    assert_eq!(game.failures(), refused.map(Some));
    assert_eq!(game.field("kind"), text("hero-x,hero-y"));
    game.tick(&[(2, probe("read"))]);
    assert_eq!(game.field("kind"), text("hero-y,hero-x"));
    // A grant puts the spell after the hero's basic ability, learned as its kind has no ranks;
    // a kind of other ranks, a kind the mode does not declare and an action that is no loadout
    // entry fail.
    game.tick(&[
        (0, probe("grant")),
        (0, probe("grant_basic")),
        (0, probe("grant_ultimate")),
        (0, probe("grant_stranger")),
    ]);
    let refused = [
        ApiError::SlotKindRanks,
        ApiError::UnknownSlotKind,
        ApiError::UnknownAction,
    ];
    assert_eq!(game.failures(), refused.map(Some));
    assert_eq!(
        slots(&game),
        [(game.strike, basic, 0), (game.blink, spell, 1)]
    );
}

#[test]
fn a_player_modifier_holds_on_each_unit_of_its_player_it_selects_and_on_one_spawned_after() {
    let script = r#"
fn on_mode_input(ctx, player, name, value) {
    let at = ctx.map.markers("camp")[0].pos;
    if value == "units" {
        ctx.spawn_unit("grunt", "a", at, 1);
        ctx.spawn_unit("tower", "a", at, 1);
        ctx.spawn_unit("grunt", "a", at, 0);
    } else if value == "drill" {
        ctx.add_player_modifier(1, "drill");
    } else if value == "another" {
        ctx.spawn_unit("grunt", "a", at, 1);
    } else if value == "stranger" {
        ctx.add_player_modifier(1, "march");
    } else if value == "nobody" {
        ctx.add_player_modifier(9, "drill");
    }
}
"#;
    let mut game = Game::new(script, LIMITS);
    let drill = Stats::modifier(&game.world, 0, "drill").unwrap();
    // Units 1 to 3, after the map's tower: a grunt and a tower of player 1, and a grunt of
    // player 0.
    game.tick(&[(1, input("probe", "units"))]);
    let drilled = |game: &Game| {
        let mut held = Vec::new();
        for (id, entity) in game.world.resource::<EntityIndex>().iter() {
            let modifiers = game.world.get::<Modifiers>(entity).unwrap();
            if let Some(instance) = modifiers.get(drill, None) {
                assert!(
                    instance.lifetime.held_by(Hold::Held) && instance.lifetime.until().is_none()
                );
                held.push(id.get());
            }
        }
        held
    };
    assert!(drilled(&game).is_empty());
    // From the tick player 1 holds the drill, its grunt holds it, from no source, with no end;
    // its tower is no grunt, and player 0's grunt is not its.
    game.tick(&[(1, input("probe", "drill"))]);
    assert_eq!(drilled(&game), [1]);
    // A grunt of player 1 spawned later holds it from its first Resolve.
    game.tick(&[(1, input("probe", "another"))]);
    assert_eq!(drilled(&game), [1, 4]);
    let held = game.world.resource::<PlayerModifiers>();
    assert_eq!(held.of(PlayerSlot::new(1)).collect::<Vec<_>>(), [drill]);
    assert_eq!(held.of(PlayerSlot::new(0)).count(), 0);
    // A modifier the package does not declare, and a player the session does not have, fail.
    game.tick(&[
        (1, input("probe", "stranger")),
        (1, input("probe", "nobody")),
    ]);
    let refused = [ApiError::UnknownModifier, ApiError::UnknownPlayer];
    assert_eq!(game.failures(), refused.map(Some));
}

/// Each state type's restore check lets the match's own values through, and refuses one that
/// names what the match lacks or has another shape than the mode's.
#[test]
fn a_restore_check_refuses_what_the_match_lacks() {
    let mut game = Game::new(SCRIPT, LIMITS);
    let grunt = game.entity(2);
    let world = &game.world;
    // Teams a, b and neutral are 0 to 2, players 0 to 2.
    assert!(Team::new(2).check(world, grunt) && !Team::new(3).check(world, grunt));
    let owner = |slot| Owner::new(PlayerSlot::new(slot));
    assert!(owner(2).check(world, grunt) && !owner(3).check(world, grunt));
    let own_type = *world.get::<UnitType>(grunt).unwrap();
    assert!(own_type.check(world, grunt) && !UnitType::new(u16::MAX).check(world, grunt));
    let mut relations = Relations::default();
    relations.set(Team::new(0), Team::new(2), Attitude::Friendly, true);
    assert!(relations.check(world));
    relations.set(Team::new(0), Team::new(3), Attitude::Friendly, true);
    assert!(!relations.check(world));
    let ended = |team| MatchEnd::new(Tick::new(0), MatchResult::Won(Team::new(team)));
    assert!(ended(1).check(world) && !ended(3).check(world));
    // The map has one path, 0.
    let on_path = |path| OnPath::new(PathId::new(path));
    assert!(on_path(0).check(world, grunt) && !on_path(1).check(world, grunt));

    // Choices: a row of 1 + 1 + 2 values for each of 3 players, each an offer of its choice.
    let choices = world.resource::<Choices>().clone();
    assert!(choices.check(world));
    assert!(!Choices(vec![None; 3]).check(world));
    let mut past = choices.clone();
    past.0[0] = Some(Offer::new(99));
    assert!(!past.check(world));
    // The mode's state: its fields' types in the order of their names, `count` an integer first.
    let state = world.resource::<ModeState>().clone();
    assert!(state.check(world));
    assert!(!ModeState(state.0[1..].to_vec()).check(world));
    let mut retyped = state.clone();
    retyped.0[0] = StateValue::Bool(true);
    assert!(!retyped.check(world));
    // The players' resources: a row of each of the mode's resources for each player.
    assert!(world.resource::<PlayerResources>().check(world));
    let rows = |players, resources| PlayerResources::new(players, resources).check(world);
    assert!(!rows(2, RESOURCES.len()) && !rows(3, RESOURCES.len() + 1));

    // Actions: one the book holds, at a rank it has or 0, and an order of a slot it has.
    let book = world.resource::<ActionBook>();
    let strike = book.action_named(0, "strike").unwrap();
    let slots = |action, rank| ActionSlots::new([(action, SlotKind::new(0), rank)]);
    assert!(slots(strike, 0).check(world, grunt) && slots(strike, 1).check(world, grunt));
    assert!(!slots(strike, u8::MAX).check(world, grunt));
    assert!(!slots(ActionId::nth(u32::MAX), 1).check(world, grunt));
    let mut ordered = slots(strike, 1);
    ordered.order(1, ActionTarget::None);
    assert!(!ordered.check(world, grunt));
    let mut queue = TrainQueue::default();
    let queued = Queued {
        action: strike,
        rank: 1,
        time: Ticks::new(1),
    };
    queue.push(queued, Tick::new(0));
    assert!(
        !queue.check(world, grunt),
        "a cast is no train, and a grunt trains none"
    );

    // Tracks: the mode has two, 0 and 1.
    let on_tracks = |track| Experience::new(TrackSet::of([TrackId::new(track).unwrap()]), None);
    assert!(on_tracks(1).check(world, grunt) && !on_tracks(2).check(world, grunt));
    // Track 0 is the `level` track, whose level is the unit's: one that holds its own is
    // refused, one that holds none passes.
    let level_track = |own: bool| {
        let track = TrackId::new(0).unwrap();
        Experience::new(TrackSet::of([track]), (!own).then_some(track))
    };
    assert!(level_track(false).check(world, grunt) && !level_track(true).check(world, grunt));
    let unit = *world.get::<StableId>(grunt).unwrap();
    let level_up = |track| LevelUp {
        unit,
        track: TrackId::new(track).unwrap(),
        level: Level::new(2).unwrap(),
    };
    assert!(LevelUps(vec![level_up(1)]).check(world));
    assert!(!LevelUps(vec![level_up(2)]).check(world));

    // A modifier of the book, with a value for its one change and no state, as the fighter's is;
    // not one the book lacks, nor one of other state or another count of changes.
    let fighter = game.fighter(0, &[("armor", num(1))]);
    let fighter = game.entity(fighter.get());
    let world = &game.world;
    let modifiers = world.get::<Modifiers>(fighter).unwrap().clone();
    assert!(modifiers.check(world, fighter));
    let modifier = modifiers.iter().next().unwrap().id;
    let changed = |change: &dyn Fn(&mut Instance)| {
        let mut changed = modifiers.clone();
        change(changed.get_mut(modifier, None).unwrap());
        changed.check(world, fighter)
    };
    assert!(!changed(&|instance| instance.id = ModifierId::new(u16::MAX)));
    assert!(!changed(
        &|instance| instance.state = vec![StateValue::Bool(true)]
    ));
    assert!(!changed(&|instance| instance.stats.clear()));
    let mut held = PlayerModifiers::default();
    held.add(PlayerModifier {
        player: PlayerSlot::new(0),
        modifier: ModifierId::new(0),
    });
    assert!(held.check(world));
    held.add(PlayerModifier {
        player: PlayerSlot::new(0),
        modifier: ModifierId::new(u16::MAX),
    });
    assert!(!held.check(world));

    // A route of the grunt, of the mode's one kind of walker, until its body grows past it.
    let route = game.world.get::<Route>(grunt).unwrap().clone();
    assert!(route.check(&game.world, grunt));
    let wide = Body::new(num(3)).unwrap();
    game.world.entity_mut(grunt).insert(wide);
    assert!(!route.check(&game.world, grunt));
    assert!(!Destination::default().check(&game.world, grunt));
}
