use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};
use std::slice;

use bevy_ecs::query::With;
use campfire_common::Ticks;
use campfire_math::{Num, Vec3};
use campfire_script::{Budget, ScriptHost, ScriptId};
use campfire_sim::{Capability, Position, SimComponent, SimResource, StableId, TickInput};

use super::*;
use crate::actions::Actions;
use crate::actions::action_book::ActionBook;
use crate::actions::action_data::{ActionData, Targeting};
use crate::actions::action_kind::ActionKind;
use crate::actions::action_target::ActionTarget;
use crate::actions::slot_kind::SlotKind;
use crate::actions::slot_kinds::{SlotKindData, SlotKinds};
use crate::capability_set::test_match::TestMatch;
use crate::combat::combat_rules::CombatRules;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::heal::{Heal, HealCause};
use crate::combat::on_death::OnDeath;
use crate::combat::pass_queue::PassQueue;
use crate::combat::recent_attackers::RecentAttackers;
use crate::mode::choice_data::{ChoiceData, Offers};
use crate::mode::error::ModeError;
use crate::mode::map_data::{GridData, MapData, MapPoint, MarkerData, PathData, PlacedUnitData};
use crate::mode::match_end::MatchResult;
use crate::mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
use crate::mode::mode_setup::{LoadoutSetup, SlotAction, UnitTypeSetup};
use crate::mode::mode_state_decl::{ModeStateDecl, SyncTo};
use crate::mode::mode_units::ModeUnits;
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
use crate::scripts::call_start::CallStart;
use crate::scripts::error::ApiError;
use crate::scripts::error::internals::FailureKind;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::state_decl::{StateDecl, StateDefault, StateType};
use crate::scripts::state_value::StateValue;
use crate::stats;
use crate::stats::Stats;
use crate::stats::application::{Application, NewInstance};
use crate::stats::instance::StatShare;
use crate::stats::level::Level;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifiers::Modifiers;
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
use crate::values::package_path::PackagePath;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;
use crate::vision::vision_grid::VisionGrid;

/// 10 ticks a second: 100 ms is a tick.
const RATE: TickRate = TickRate::new(NonZeroU32::new(10).unwrap());

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

/// `on_mode_input` that picks the hero a `hero` input names, as `PICK` does, and passes any other
/// input to the script's own `on_input`.
const PICKING: &str = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
        return;
    }
    on_input(ctx, player, name, value);
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
/// A marker `name` with `tag` at (`x`, `z`), of `team` if it names one, with `params`.
fn marker(
    name: &str,
    tag: &str,
    (x, z): (i64, i64),
    team: Option<&str>,
    params: &[(&str, ModeParam)],
) -> MarkerData {
    MarkerData {
        team: team.map(|name| DeclaredName::new(name).unwrap()),
        params: params
            .iter()
            .map(|(name, param)| (DeclaredName::new(name).unwrap(), param.clone()))
            .collect(),
        ..MarkerData::tagged(name, &[tag], MapPoint::ground(x, z))
    }
}

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap()
}

fn field(kind: StateType, default: Option<StateDefault>) -> StateDecl {
    StateDecl::new(kind, default).unwrap()
}

/// The unit kit of a grunt: 10 health, combat that keeps it when it dies, and a step of 1 m.
fn grunt() -> UnitKit {
    UnitKit {
        pools: Some(Pools::life(Num::int(10))),
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
        levels: Thresholds::new(levels.iter().map(|&value| Num::int(value))).unwrap(),
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
    ActionData::cast(Targeting::None)
}

/// Bounds from (−10, −5) to (10, 6) with a grid of 1 m cells; one path, `mid`, along x; team a's
/// spawn at z = −5 and b's at 5; a's tower 8 m down the path; and a camp of a grunt in the middle.
fn map() -> MapData {
    let grunt = ("unit_type", ModeParam::Text("grunt".to_owned()));
    MapData {
        grid: Some(GridData {
            cell: Scalar::Int(1),
        }),
        navigation: Some(GridData {
            cell: Scalar::Int(1),
        }),
        paths: vec![PathData {
            name: DeclaredName::new("mid").unwrap(),
            points: [-10, 0, 10].map(|x| MapPoint::ground(x, 0)).into(),
        }],
        units: vec![PlacedUnitData {
            path: Some(DeclaredName::new("mid").unwrap()),
            ..PlacedUnitData::new("tower", "a", MapPoint::ground(-8, 0))
        }],
        markers: vec![
            marker("a_spawn", "spawn", (0, -5), Some("a"), &[]),
            marker("b_spawn", "spawn", (0, 5), Some("b"), &[]),
            marker("camp", "camp", (0, 0), None, slice::from_ref(&grunt)),
        ],
        ..MapData::planar(
            Bounds::new([Num::int(-10), Num::int(-5)], [Num::int(10), Num::int(6)]).unwrap(),
        )
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
        duration_ms: Some(Number::Value(Scalar::Int(200))),
        reapply: Reapply::Stack,
        max_stacks: NonZeroU32::new(4),
        state: [(
            DeclaredName::new("count").unwrap(),
            field(StateType::Int, None),
        )]
        .into(),
        ..ModifierData::default()
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
    let hero = UnitKit {
        tracks: TrackSet::of([0, 1].map(|at| TrackId::new(at).unwrap())),
        ..grunt()
    };
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
        units: ModeUnits {
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
                        body: Body::new(Num::int(1)).map(|body| body.on(Layer::new(1))),
                        ..grunt()
                    },
                ),
            ],
            avatars: ["hero-x", "hero-y"].into_iter().collect(),
            loadout: spell,
        },
        walkers: vec![Walker::of(grunt().body.as_ref())],
    }
}

#[derive(Debug)]
struct Game {
    sim: TestMatch,
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

    /// A match of `script`, whose `on_input` takes every mode input but a pick of a hero.
    fn picking(script: &str, limits: ScriptLimits) -> Game {
        Game::new(&format!("{script}{PICKING}"), limits)
    }

    /// Player `slot` picks `hero` in a tick: the hero it then owns.
    fn pick(&mut self, slot: u32, hero: &str) -> Entity {
        self.tick(&[(slot, input("hero", hero))]);
        let player = PlayerSlot::new(slot);
        let mut owned = self.sim.world.query::<(Entity, &Owner)>();
        let mut heroes = owned
            .iter(&self.sim.world)
            .filter(|(_, owner)| owner.slot() == player);
        let (hero, _) = heroes.next().expect("the player picked a hero");
        assert!(heroes.next().is_none(), "the player owns one hero");
        hero
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
        let mut sim = TestMatch::new(&declared, RATE, Some(scripts));
        let world = &mut sim.world;
        let resources: Vec<&str> = files
            .data
            .resources
            .iter()
            .map(DeclaredName::as_str)
            .collect();
        Units::name_kinds(world, &DAMAGE_KINDS, &POOLS, &resources);
        let layers = files.data.navigation.layers.iter();
        Units::declare_tags(world, layers.map(DeclaredName::as_str));
        let mut load = |name: &str, tag: &str| {
            let data = UnitTypeData::tagged(&[tag]);
            Units::load_type(world, TypeScope::Mode, name, &data)
        };
        let (grunt_type, tower_type) = (load("grunt", "grunt"), load("tower", "tower"));
        let (x, y) = (load("hero-x", "avatar"), load("hero-y", "avatar"));
        // A type outside the mode's kits, as a projectile's is: the view knows it, a spawn does
        // not.
        load("bolt", "projectile");
        // The stats first, as a match's books know them before any action or modifier; the mode's
        // books give the full book at install.
        stats::loads::load_stats(world, &files.data.stats);
        let blink = blink_data();
        // A spell has one rank; hero X's ability, 2.
        let strike = Actions::load(world, 0, "strike", &blink, None, 2).unwrap();
        let blink = Actions::load(world, 0, "blink", &blink, None, 1).unwrap();
        let mut spell = LoadoutSetup::default();
        spell.push("blink", blink);
        let types = [grunt_type, tower_type, x, y];
        Progression::load(world, &files.data.tracks);
        for (name, data) in &files.modifiers {
            Stats::load_modifier(world, 0, name, data, None);
        }
        let script = Units::compile_hooked(world, &format!("{script}{PICK}")).unwrap();
        let blessing = Stats::modifier(world, 0, "blessing").unwrap();
        let setup = setup(&files, script, types, spell, strike, blessing);
        let books = {
            let view = world.non_send::<View>();
            let stats = StatBook::new(&files.data.stats, [], Num::int(10));
            let relations = &files.data.relations;
            let unit_type = |name: &str| view.unit_type_named(name);
            let map = ModeMap::resolve(&files.map, &files.teams, relations, unit_type).unwrap();
            let unit_types = &setup.units.unit_types;
            ModeBooks::build(&files.data, unit_types, &mut view.types_mut(), stats, map)
        };
        sim.install(|world, schedule, registry| {
            Mode::install(world, schedule, registry, setup, books);
        });
        // The scripts name pools the data does not declare, whose maxima no stat sets.
        Units::name_kinds(&sim.world, &DAMAGE_KINDS, &POOLS, &RESOURCES);
        Mode::start(&mut sim.world)?;
        Ok(Game { sim, blink, strike })
    }

    /// Runs a tick with `inputs`, each a player's slot and a mode input.
    fn tick(&mut self, inputs: &[(u32, ModeInput<'_>)]) {
        for (slot, input) in inputs {
            let payload = ModeInput::payload(slice::from_ref(input));
            self.sim.world.resource_mut::<TickInputs>().push(TickInput {
                slot: PlayerSlot::new(*slot),
                payload: &payload,
            });
        }
        self.sim.step();
    }

    /// Unit `id`.
    fn entity(&self, id: u64) -> Entity {
        let mut units = self.sim.world.resource::<EntityIndex>().iter();
        units.find(|(unit, _)| unit.get() == id).unwrap().1
    }

    /// The state fields `phase`, `seen`, `count` and `inputs`.
    fn state(&self) -> [StateValue; 4] {
        ["phase", "seen", "count", "inputs"].map(|name| self.field(name))
    }

    /// The state field `name`: the state holds the fields in the order of their names.
    fn field(&self, name: &str) -> StateValue {
        let ctx = self.sim.world.non_send::<Ctx>();
        let at = ModeBook::of(ctx)
            .unwrap()
            .schema
            .state_field_named(name)
            .unwrap()
            .index;
        self.sim.world.resource::<ModeState>().get()[at].clone()
    }

    /// Each unit: its id, where it stands, its team, and the end of a path it walks from.
    fn units(&self) -> Vec<(u64, Position, u8, Option<PathEnd>)> {
        let world = &self.sim.world;
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

    /// The kind of each of the tick's failed calls.
    fn failures(&self) -> Vec<FailureKind> {
        let failures = self.sim.world.non_send::<ScriptFailures>().calls();
        failures.into_iter().map(|failure| failure.kind).collect()
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

/// The reference 3v3's `source_stat` and `calc_damage` as they were when these tests were written:
/// the engine's tests keep their own copy, so a balance change to the 3v3 changes none of them.
const CALC_DAMAGE_3V3: &str = r#"
// A source that is gone, as from a projectile that outlived it, has no bonus and no penetration.
fn source_stat(d, name) {
    let source = d.source;
    if source == () { 0 } else { source.stat(name) }
}

fn calc_damage(ctx, d) {
    let amount = d.amount * (1 + source_stat(d, "damage_dealt_pct"));
    if d.roll != () && d.roll < source_stat(d, "crit_chance") {
        amount *= 2;
    }
    if d.kind == "true" {
        return amount;
    }
    let physical = d.kind == "physical";
    let resist = if physical { d.target.stat("armor") } else { d.target.stat("magic_resist") };
    let pen_pct = source_stat(d, if physical { "armor_pen_pct" } else { "magic_pen_pct" });
    let pen_flat = source_stat(d, if physical { "armor_pen" } else { "magic_pen" });
    if resist > 0 {
        resist = (resist * (1 - pen_pct) - pen_flat).max(0);
    }
    if resist >= 0 {
        amount = amount * 100 / (100 + resist);
    } else {
        amount = amount * (2 - num(100) / (100 - resist));
    }
    if physical {
        amount = (amount - d.target.stat("physical_block")).max(0);
    }
    amount
}
"#;

impl Game {
    /// A grunt of 1000 health on `team`, whose modifier adds `stats` by name.
    fn fighter(&mut self, team: u8, stats: &[(&str, Num)]) -> StableId {
        let book = self.sim.world.resource::<StatBook>();
        let changes: Vec<_> = stats
            .iter()
            .map(|&(name, _)| {
                (
                    book.named(&Stat::named(name).unwrap()).unwrap(),
                    StatOp::Add,
                )
            })
            .collect();
        let mut modifiers = self.sim.world.resource_mut::<ModifierBook>();
        let id = modifiers.push_changes(&changes, TagSet::default());
        let shares = stats
            .iter()
            .map(|&(_, value)| StatShare { value, live: None });
        let instance = NewInstance {
            stats: shares.collect(),
            ..NewInstance::bare(id, None)
        };
        let modifiers = Modifiers::bundle([Application::refresh(instance)]);
        let grunt = self
            .sim
            .world
            .non_send::<View>()
            .unit_type_named("grunt")
            .unwrap();
        let id = self.sim.world.resource_mut::<IdAllocator>().allocate();
        self.sim.world.spawn((
            id,
            at(0, 0),
            Team::new(team),
            Pools::life(Num::int(1000)),
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
        self.sim
            .world
            .resource_mut::<PassQueue>()
            .push_damage(Damage {
                source,
                target,
                amount: Num::int(amount),
                kind: DamageKind::new(u8::try_from(kind).unwrap()),
                cause,
                ability: None,
                depth: 0,
                hit: None,
            });
    }
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
        let ctx = self.sim.world.non_send::<Ctx>().clone();
        ctx.view().read(&mut self.sim.world);
        match role {
            ScriptRole::Mode => ctx.frame().begin_mode(&self.sim.world, false),
            ScriptRole::Ai => ctx.frame().begin_think(&self.sim.world, actor),
            ScriptRole::Action => ctx
                .frame()
                .begin(&self.sim.world, CallStart::cast(self.strike, 1, actor, 0))
                .unwrap(),
            ScriptRole::Modifier => {
                let blessing = Stats::modifier(&self.sim.world, 0, "blessing").unwrap();
                let start = CallStart {
                    acting: Some(actor),
                    ..CallStart::hook(blessing, 0, 1)
                };
                ctx.frame().begin(&self.sim.world, start).unwrap();
            }
        }
        let handle = ctx.view().unit(unit).unwrap();
        let returned = {
            let mut host = self.sim.world.non_send_mut::<ScriptHost>();
            let script = host.compile(source).unwrap();
            let mut budget = Budget::new(u64::MAX);
            host.call(&mut budget, script, "probe", (ctx.clone(), handle))
        };
        let returned = returned.map_err(CallError::from_script)?;
        let now = self.sim.world.resource::<SimTick>().start();
        ctx.apply(&mut self.sim.world, now);
        Ok(returned)
    }
}

mod budgets;
mod damage;
mod inputs;
mod match_end;
mod modifiers;
mod production;
mod progression;
mod restore;
mod roles;
mod start;
