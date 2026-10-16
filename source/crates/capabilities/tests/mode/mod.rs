use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};
use std::slice;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionData, ActionKind, ActionSlots, ActionTarget, Actions, SlotKind, SlotKindBook,
    SlotKindData, SlotKinds, SlotRanks, Targeting, UseGroup,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_combat::{
    CombatBook, Damage, DamageCause, DamagePass, Heal, HealCause, ModeCombatData, OnDeath,
    PassEntry, RecentAttackers, Respawn,
};
use campfire_common::{PlayerSlot, Ticks};
use campfire_declare::{Book, ByType};
use campfire_geometry::{Bounds, Grid, Metric};
use campfire_items::Inventory;
use campfire_math::{Num, Vec3};
use campfire_mode::{
    InputValue, MapPoint, MarkerData, MatchResult, ModeError, ModeInput, Offer, TeamManifest,
    UnansweredSlotEvents, *,
};
use campfire_navigation::{
    ModeNavigationData, NavigationBook, OnPath, PathEnd, Route, Walker, WalkerKinds,
};
use campfire_players::{OutOfPlay, PlayerResources, PlayersWorld};
use campfire_production::{ProductionData, Queued, TrainQueue};
use campfire_progression::{Experience, Points, Progression, Thresholds, TrackData, TrackSet};
use campfire_script::internals::FailureKind;
use campfire_script::{
    ApiError, CallError, NameKind, ParamProblem, Pool, ScriptBudgets, ScriptFailures, ScriptId,
    ScriptLimits, ScriptRole, StateDecl, StateDefault, StateType, StateValue, SyncTo,
    SyncedStateDecl, View,
};
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, SimComponent, SimResource, SimTick, SlotEvent,
    SlotEventKind, StableId, TickRate,
};
use campfire_stats::{
    Application, Hold, Level, Life, ModifierClocks, ModifierData, Modifiers, NewInstance,
    PlayerModifiers, PoolBook, PoolId, Pools, Reapply, StatBook, StatRule, StatShare, Stats,
    TypePassive, UnitStats,
};
use campfire_units::{
    Body, BodyForm, Dead, Destination, Facing, HeldPlayerUnits, Layer, MatchEntity, MoveStep,
    Owner, Player, SyncAll, SyncNone, UnitTypeCore, Units, UnitsWorld,
};
use campfire_values::{
    ActionId, DamageKind, DeclaredName, EngineEnum, FilterData, MessageId, ModifierId, Names,
    Number, PackageIndex, ParamRef, PathId, Rank, Ranked, Relation, Relations, ResourceId, Scalar,
    Stat, Team, TrackId, TypeScope, UnitTags, UnitType,
};
use campfire_visibility::ModeVisibility;

/// 10 ticks a second: 100 ms is a tick.
const RATE: TickRate = TickRate::new(NonZeroU32::new(10).unwrap());

/// A mode that records what its hooks see in its state, and acts on its players' inputs.
/// The MOBA test content's damage kinds, and the stats its `calc_damage` reads.
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
    ctx.spawn_group("a", "mid", PathEnd::Start, ctx.p.group);
    ctx.spawn_group("b", "mid", PathEnd::End, ["grunt"]);
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
    } else if name == "out" {
        ctx.set_out(player, value == "on");
    } else if name == "phase" {
        ctx.state.phase = 5;
    } else if name == "markers" {
        ctx.map.markers(value);
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
            let id = ctx.spawn_unit(hero, team, marker.pos, player);
            ctx.grant(id, "spell", ctx.chosen(player, "spells"));
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

/// The MOBA 3v3's `on_unit_died`, `hero_died` and `share_xp` as they were when these tests
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
    // Only a hero's player takes gold: a tower or a creep that struck the hero takes no share.
    let heroes = [];
    for unit in assisters {
        if unit.is_avatar {
            heroes.push(unit);
        }
    }
    for assister in heroes {
        ctx.add_resource(assister.owner, "gold", ctx.p.assist_gold / heroes.len());
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

fn field(kind: StateType, default: Option<StateDefault>) -> StateDecl {
    StateDecl::new(kind, default).unwrap()
}

/// The grunt's core fields: its tag, and its script state, `hits` of 2, which only its owner
/// receives, and `mark`, a text of none, which every client that sees it receives.
fn grunt_data() -> UnitTypeCore {
    let decl = |kind, default, sync| SyncedStateDecl {
        decl: field(kind, default),
        sync,
    };
    let hits = decl(StateType::Int, Some(StateDefault::Int(2)), SyncTo::Owner);
    UnitTypeCore {
        state: [
            ("hits", hits),
            ("mark", decl(StateType::String, None, SyncTo::All)),
        ]
        .map(|(name, decl)| (DeclaredName::new(name).unwrap(), decl))
        .into(),
        ..UnitTypeCore::tagged(&["grunt"])
    }
}

/// A unit type of the test mode, as its books give it: 10 health, combat that keeps it when it
/// dies, its step, its body, its tracks, its slots, of its actions and its inventory, and its
/// passive.
#[derive(Debug, Clone)]
struct TestType {
    unit_type: UnitType,
    step: Option<MoveStep>,
    body: Option<BodyForm>,
    tracks: TrackSet,
    actions: Vec<(ActionId, SlotKind, Option<Rank>)>,
    inventory: Option<(Inventory, UseGroup)>,
    passive: Option<ModifierId>,
}

impl TestType {
    /// A grunt of `unit_type`: a step of 1 m, no body, track, slot or passive.
    fn grunt(unit_type: UnitType) -> Self {
        Self {
            unit_type,
            step: Some(MoveStep::new(Num::ONE).unwrap()),
            body: None,
            tracks: TrackSet::default(),
            actions: Vec::new(),
            inventory: None,
            passive: None,
        }
    }

    /// Its slots: its actions, then its inventory's; none with neither.
    fn slots(&self) -> Option<ActionSlots> {
        if self.actions.is_empty() && self.inventory.is_none() {
            return None;
        }
        let slots = ActionSlots::new(self.actions.iter().copied());
        Some(match &self.inventory {
            Some((inventory, group)) => inventory.extend_slots(slots, *group),
            None => slots,
        })
    }

    /// Loads the parts of `types` into `game`, each book as the install loads it.
    fn load(game: &mut Match, types: &[Self]) {
        let mut lives = ByType::default();
        let mut deaths = ByType::default();
        let mut steps = ByType::default();
        let mut bodies = ByType::default();
        let mut tracks = ByType::default();
        let mut slots = ByType::default();
        let mut passives = ByType::default();
        for unit in types {
            let unit_type = unit.unit_type;
            lives.set(unit_type, Life::full(Num::int(10)));
            deaths.set(unit_type, OnDeath::Stay);
            steps.set_some(unit_type, unit.step);
            bodies.set_some(unit_type, unit.body);
            tracks.set_some(unit_type, (!unit.tracks.is_empty()).then_some(unit.tracks));
            slots.set_some(unit_type, unit.slots());
            passives.set_some(unit_type, unit.passive.map(TypePassive));
        }
        lives.load_into(game);
        deaths.load_into(game);
        steps.load_into(game);
        bodies.load_into(game);
        tracks.load_into(game);
        slots.load_into(game);
        passives.load_into(game);
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
            cell: Num::ONE,
            brush: Vec::new(),
        }),
        navigation: Some(MapNavigationData {
            cell: Num::ONE,
            walls: Vec::new(),
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
        *point = MapPoint::Space([x, Num::int(y), z]);
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

/// A shield of the `ward` of the action that applies it, which the mode, with none, never gives.
fn rally() -> ModifierData {
    let ward = DeclaredName::new("ward").unwrap();
    ModifierData {
        shield: Some(Number::Param(ParamRef { param: ward })),
        ..ModifierData::default()
    }
}

/// The test mode's slot kinds: `basic`, of 2 ranks, and `spell`, learned from the spawn.
fn slot_kinds() -> SlotKinds {
    let kind = |name, ranks| SlotKindData {
        name: DeclaredName::new(name).unwrap(),
        ranks: NonZeroU8::new(ranks).map(|count| SlotRanks::new(count, None).unwrap()),
    };
    SlotKinds::new([kind("basic", 2), kind("spell", 0)]).unwrap()
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
            name: MessageId::new(name).unwrap(),
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
    RESOURCES
        .into_iter()
        .collect::<Names<ResourceId>>()
        .named(name)
}

/// The mode's state fields: `kind` and `seen` kept on the server, every other sent to all.
fn mode_state() -> BTreeMap<DeclaredName, SyncedStateDecl> {
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
        let sync = if matches!(name, "kind" | "seen") {
            SyncTo::None
        } else {
            SyncTo::All
        };
        (
            DeclaredName::new(name).unwrap(),
            SyncedStateDecl { decl, sync },
        )
    })
    .into()
}

fn mode_files() -> ModeFiles {
    let text = |text: &str| ListEntry::Text(text.to_owned());
    ModeFiles {
        data: ModeData {
            name: MessageId::new("mode").unwrap(),
            teams: BTreeMap::new(),
            combat: ModeCombatData {
                damage_kinds: DAMAGE_KINDS
                    .map(|kind| DeclaredName::new(kind).unwrap())
                    .into(),
                ..ModeCombatData::default()
            },
            navigation: ModeNavigationData {
                layers: ["ground", "air"]
                    .map(|name| DeclaredName::new(name).unwrap())
                    .into(),
                spawn_tags: Vec::new(),
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
                ("out", InputType::String),
                ("markers", InputType::String),
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
                relation: Relation::Neutral,
                vision: true,
            }],
            tags: BTreeMap::new(),
            tracks: tracks(),
            players: PlayersData::default(),
            saves: SavesData::default(),
            supply: None,
            visibility: ModeVisibility::default(),
        },
        modifiers: [
            ("blessing".to_owned(), blessing()),
            ("drill".to_owned(), drill()),
            ("rally".to_owned(), rally()),
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

/// The test mode's unit types: its grunt, tower, two heroes' and crate's, the heroes on both
/// tracks, hero X with one ability, `strike`, and hero Y with the passive `blessing`, the tower a
/// circle of 1 m on the second layer and the crate a box of 2 by 2 m, neither of which walks.
fn test_types(types: [UnitType; 5], strike: ActionId, blessing: ModifierId) -> Vec<TestType> {
    let [grunt, tower, x, y, crate_type] = types.map(TestType::grunt);
    let hero = |grunt: TestType| TestType {
        tracks: TrackSet::from([0, 1].map(TrackId::new)),
        ..grunt
    };
    vec![
        grunt,
        TestType {
            actions: vec![(strike, SlotKind::new(0), None)],
            ..hero(x)
        },
        TestType {
            passive: Some(blessing),
            ..hero(y)
        },
        TestType {
            step: None,
            body: BodyForm::circle(Num::int(1)).map(|form| form.on(Layer::new(1))),
            ..tower
        },
        TestType {
            step: None,
            body: BodyForm::box_sized([Num::int(2), Num::int(2)]),
            ..crate_type
        },
    ]
}

/// The test mode's setup from `files`, of `script`, of the unit `types`, with its one spell.
fn setup<'a>(
    files: &'a ModeFiles,
    script: ScriptId,
    types: &[TestType],
    spell: LoadoutSetup,
) -> ModeSetup<'a> {
    let unit_types = types.iter().map(|unit| unit.unit_type);
    ModeSetup {
        script,
        data: &files.data,
        teams: &files.teams,
        players: 3,
        units: ModeUnits {
            unit_types: unit_types.collect(),
            avatars: Names::from(["hero-x", "hero-y"]),
            loadout: spell,
        },
    }
}

/// A match of the test mode for 3 players, two on team `a` and one on `b`, with `limits`.
fn mode_match(script: &str, limits: ScriptLimits) -> Match {
    start_match(script, limits, &mode_files()).unwrap()
}

/// A match of `script`, whose `on_input` takes every mode input but a pick of a hero.
fn picking_match(script: &str, limits: ScriptLimits) -> Match {
    mode_match(&format!("{script}{PICKING}"), limits)
}

/// The match `mode_match` gives, of the mode `files`; an error when the mode's start fails.
fn start_match(script: &str, limits: ScriptLimits, files: &ModeFiles) -> Result<Match, CallError> {
    start_match_setup(script, limits, files, |_| {})
}

/// Loads the books of the mode of `files` and its unit `types` into `game`, each as the install
/// loads it, and gives back what `Mode::load` takes beside them.
fn load_mode_books(game: &mut Match, files: &ModeFiles, types: &[TestType]) -> ModeLoad {
    // The grunt and both heroes walk with the grunt's body: one kind.
    let mut walkers = ByType::default();
    for unit in types {
        if unit.step.is_some() {
            walkers.set(unit.unit_type, Walker::of_form(unit.body).unwrap());
        }
    }
    // The mode's books, each loaded as the install loads it.
    let data = &files.data;
    let mode_names = ModeNames::of(data).unwrap();
    let resolved = {
        let view = game.non_send::<View>();
        let relations = &data.relations;
        let unit_type = |name: &str| view.names().named((TypeScope::Mode, name));
        let layers = &mode_names.layers;
        let input = MapInput {
            map: &files.map,
            heights: None,
            teams: &files.teams,
            relations,
            layers,
        };
        ModeMap::resolve(input, unit_type).unwrap()
    };
    let stats = StatBook::new(&data.stats, Num::int(10));
    let stat = |stat: &Stat| stats.named(stat).ok_or_else(|| stat.clone());
    // The test mode names no life pool: the one the test match binds stays.
    let life = data.combat.life_pool(&mode_names.pools).unwrap();
    let life = life.or_else(|| game.resource::<PoolBook>().life());
    let pools = Arc::clone(&mode_names.pools);
    let pools = PoolBook::new(&data.pools, pools, life, |_, held| stat(held)).unwrap();
    let combat = CombatBook::new(&data.combat, RATE, stat).unwrap();
    Units::change_types(game, |_, unit_types| {
        let bodies = types.iter().map(|unit| (unit.unit_type, unit.body));
        unit_types.tag_layers(&mode_names.layers, bodies);
    });
    Units::seal_types(game, &data.tags);
    stats.load_into(game);
    pools.load_into(game);
    TestType::load(game, types);
    SlotKindBook(Arc::new(data.slots.clone())).load_into(game);
    combat.load_into(game);
    let ResolvedMap {
        map,
        ground,
        navigation,
        vision,
    } = resolved;
    ground.load_into(game);
    navigation.load_into(game);
    vision.load_into(game);
    {
        let view = game.non_send::<View>();
        let mut names = view.names_mut();
        names.set_table(mode_names.damage_kinds);
        names.set_table(mode_names.resources);
    }
    let kinds: WalkerKinds = walkers.values().copied().collect();
    assert_eq!(kinds, WalkerKinds::from([Walker::of_form(None).unwrap()]));
    ModeLoad {
        map,
        walkers: kinds,
        loadout_ranks: data.loadout_ranks(),
    }
}

/// The match `start` gives, its setup changed by `adjust` before the mode installs, every slot
/// held.
fn start_match_setup(
    script: &str,
    limits: ScriptLimits,
    files: &ModeFiles,
    adjust: impl FnOnce(&mut [TestType]),
) -> Result<Match, CallError> {
    let mut game = installed_match(script, limits, files, adjust);
    Mode::start_held(&mut game)?;
    Ok(game)
}

/// The match `start_match_setup` gives, before its start.
fn installed_match(
    script: &str,
    limits: ScriptLimits,
    files: &ModeFiles,
    adjust: impl FnOnce(&mut [TestType]),
) -> Match {
    let scripts = ScriptBudgets::new(limits, 3);
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Vision,
        Capability::Abilities,
        Capability::Progression,
        Capability::Production,
    ];
    let mut game = Match::test_unstarted(&declared, RATE, Some(scripts));
    let resources: Vec<&str> = files
        .data
        .resources
        .iter()
        .map(DeclaredName::as_str)
        .collect();
    Units::name_kinds(&game, &DAMAGE_KINDS, &POOLS, &resources);
    // The mode's tags, the layers' then those of its data, as its packages name them.
    let layers = files.data.navigation.layers.iter();
    let tags = layers.chain(files.data.tags.keys());
    Units::declare_tags(&mut game, tags.map(DeclaredName::as_str));
    let grunt_type = Units::load_type(&mut game, TypeScope::Mode, "grunt", &grunt_data());
    let mut load = |name: &str, tag: &str| {
        let data = UnitTypeCore::tagged(&[tag]);
        Units::load_type(&mut game, TypeScope::Mode, name, &data)
    };
    let tower_type = load("tower", "tower");
    let (x, y) = (load("hero-x", "avatar"), load("hero-y", "avatar"));
    let crate_type = load("crate", "crate");
    // A type the mode does not spawn, as a projectile's is: the view knows it, a spawn does
    // not.
    load("bolt", "projectile");
    // The stats first, as a match's books know them before any action or modifier; the mode's
    // books give the full book at install.
    Stats::load_rules(&mut game, &files.data.stats);
    let blink = blink_data();
    // A spell has one rank; hero X's ability, 2.
    let strike = Actions::load(&mut game, PackageIndex::MODE, "strike", &blink, None, 2).unwrap();
    let blink = Actions::load(&mut game, PackageIndex::MODE, "blink", &blink, None, 1).unwrap();
    // Loaded out of name order, each is found by its name, and only in its package.
    let named =
        ["blink", "strike", "dash"].map(|name| Actions::named(&game, PackageIndex::MODE, name));
    assert_eq!(named, [Some(blink), Some(strike), None]);
    assert_eq!(Actions::named(&game, PackageIndex::new(1), "blink"), None);
    let mut spell = LoadoutSetup::default();
    spell.push("blink", blink);
    let types = [grunt_type, tower_type, x, y, crate_type];
    Progression::load_tracks(&mut game, &files.data.tracks);
    for (name, data) in &files.modifiers {
        Stats::load_modifier(&mut game, PackageIndex::MODE, name, data, None);
    }
    let script = Units::compile_hooked(&mut game, &format!("{script}{PICK}")).unwrap();
    let blessing = Stats::modifier(&game, PackageIndex::MODE, "blessing").unwrap();
    let mut types = test_types(types, strike, blessing);
    adjust(&mut types);
    let setup = setup(files, script, &types, spell);
    let load = load_mode_books(&mut game, files, &types);
    Release::install_mode(&mut game, setup, load);
    // The scripts name pools the data does not declare, whose maxima no stat sets.
    Units::name_kinds(&game, &DAMAGE_KINDS, &POOLS, &RESOURCES);
    game
}

const fn input<'a>(name: &'a str, value: &'a str) -> ModeInput<'a> {
    ModeInput {
        name,
        value: InputValue::String(value),
    }
}

/// `phase`, `seen`, `count` and `inputs`.
fn state(phase: &str, seen: i64, count: i64, inputs: i64) -> [StateValue; 4] {
    let [seen, count, inputs] = [seen, count, inputs].map(StateValue::Int);
    [StateValue::Text(phase.into()), seen, count, inputs]
}

/// The MOBA 3v3's `source_stat` and `calc_damage` as they were when these tests were written:
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

mod budgets;
mod cooldowns;
mod damage;
mod draws;
mod inputs;
mod match_end;
mod modifiers;
mod production;
mod progression;
mod restore;
mod roles;
mod start;
mod timers;
mod unit_state;
