use std::fmt::Write as _;
use std::num::NonZeroU32;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionSlots, ActionTarget, Actions, SlotKind};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_combat::{
    Damage, DamageCause, DamagePass, Heal, HealCause, PassEntry, RecentAttackers, Respawn,
};
use campfire_common::{DeclaredName, PlayerSlot, Ticks};
use campfire_geometry::{Grid, Metric};
use campfire_math::{Num, Vec3};
use campfire_mode::{
    InputValue, MatchResult, ModeError, ModeInput, Offer, UnansweredPlayerSlotEvents, *,
};
use campfire_navigation::{NavigationBook, OnPath, PathEnd, Route};
use campfire_players::{MatchEntity, OutOfPlay, Player, PlayerResources, PlayersWorld};
use campfire_production::{Queued, TrainQueue};
use campfire_progression::{Experience, Points, TrackSet};
use campfire_script::{
    ApiError, CallError, FailureKind, ListPlace, MatchNames, ParamProblem, Pool, ScriptBudgets,
    ScriptFailures, ScriptLimits, ScriptRole, StateValue,
};
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, MatchRate, PlayerSlotEvent, PlayerSlotEventKind,
    SimComponent, SimResource, SimTick, StableId,
};
use campfire_stats::{
    Application, Hold, Level, ModifierClocks, Modifiers, NewInstance, PlayerModifiers, Pools,
    Reapply, StatBook, StatShare, Stats, UnitStats,
};
use campfire_units::internals::Spawn;
use campfire_units::{
    AudienceAll, AudienceNone, Body, Dead, Destination, Facing, HeldPlayerUnits, Life, MapBook,
    MapPoint, MetricData, MoveStep, Owner, PoolId, Units, UnitsWorld,
};
use campfire_values::{
    ActionId, DamageKind, ModifierId, Names, PackageIndex, PathId, Rank, Relation, Relations,
    ResourceId, ScriptEnum, Team, TrackId, TypeScope, UnitTags, UnitType,
};
use campfire_vision::VisionBook;

/// 10 ticks a second: 100 ms is a tick.
const RATE: MatchRate = MatchRate::new(NonZeroU32::new(10).unwrap());

/// The MOBA test content's damage kinds, which the test mode declares.
const DAMAGE_KINDS: [&str; 3] = ["physical", "magic", "true"];
/// Hero X's package, the first the test mode depends on, after its own.
const HERO_X_PACKAGE: PackageIndex = PackageIndex::new(1);
/// The package of the test mode's spells, after its own and its two heroes'.
const SPELLS: PackageIndex = PackageIndex::new(3);
/// The players' resources the mode declares.
const RESOURCES: [&str; 2] = ["gold", "gems"];
const MANA: PoolId = PoolId::new(1).unwrap();

/// A mode that records what its hooks see in its state, and acts on its players' inputs.
const SCRIPT: &str = r#"
fn on_match_start(ctx) {
    ctx.state.phase = "start";
    ctx.after("once", 250, 7);
    ctx.every("every", 100);
    for camp in ctx.map.markers("camp") {
        ctx.spawn_unit(camp.params.unit_type, "neutral", camp.position);
    }
    ctx.spawn_group("a", "mid", PathEnd::Start, ctx.params.group);
    ctx.spawn_group("b", "mid", PathEnd::End, ["grunt"]);
}

fn on_timer(ctx, name, data) {
    if name == "once" {
        ctx.state.seen = data;
    } else {
        ctx.state.count += 1;
    }
}

fn on_input(ctx, player, name, value) {
    ctx.state.inputs += 1;
    if name == "hero" {
        pick(ctx, player, value);
    } else if name == "spells" {
        ctx.choose(player, "spells", value);
    } else if name == "rich" {
        ctx.add_resource(player, value, 9223372036854775807);
    } else if name == "gold" {
        ctx.add_resource(player, value, ctx.params.gold);
        ctx.add_resource(player, value, ctx.params.bounty);
    } else if name == "fail" {
        ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].position);
        throw value;
    } else if name == "out" {
        ctx.set_out(player, value == "on");
    } else if name == "phase" {
        ctx.state.phase = 5;
    } else if name == "markers" {
        ctx.map.markers(value);
    } else if name == "probe" {
        let grunts = ctx.all_units_tagged("grunt");
        ctx.state.enemy = ctx.enemies("a")[0];
        ctx.state.grunts = grunts.len() * 10 + ctx.units_tagged("grunt").len();
        ctx.state.heroes = ctx.avatars("b").len();
        ctx.state.teams = ctx.teams.len();
        ctx.state.players = ctx.players.len();
        ctx.state.path = grunts[1].path;
        ctx.state.team = grunts[1].team;
        ctx.state.neutral = grunts[0].team;
        ctx.state.owner = ctx.avatars()[0].owner.player_slot;
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
    let team = player.team;
    for marker in ctx.map.markers("spawn") {
        if marker.team == team {
            let id = ctx.spawn_unit(hero, team, marker.position, player);
            ctx.grant(id, "spell", ctx.chosen(player, "spells"));
        }
    }
}
"#;

/// `on_input` that picks the hero a `hero` input names, as `PICK` does, and passes any other
/// input to the script's own `take_input`.
const PICKING: &str = r#"
fn on_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
        return;
    }
    take_input(ctx, player, name, value);
}
"#;

/// The MOBA 3v3's `on_unit_death`, `hero_died` and `share_xp` as they were when these tests
/// were written.
const DEATHS_3V3: &str = r#"
fn on_unit_death(ctx, unit, killer, assisters) {
    if unit.has_tag("core") {
        ctx.win(ctx.enemies(unit.team)[0]);
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
        ctx.respawn(unit, ctx.params.inhibitor_respawn_ms);
    } else if unit.has_tag("objective") {
        if killer != () {
            for hero in ctx.avatars(killer.team) {
                ctx.add_modifier(hero, "warden_blessing");
            }
        }
        ctx.respawn(unit, ctx.params.warden_respawn_ms);
    } else if unit.has_tag("camp") {
        ctx.respawn(unit, ctx.params.camp_respawn_ms);
    }
}

fn hero_died(ctx, hero, killer, assisters) {
    ctx.respawn(hero, ctx.params.respawn_base_ms + ctx.params.respawn_per_level_ms * hero.level);
    if killer == () || !killer.is_avatar {
        return;
    }
    let gold = ctx.params.kill_gold;
    if !ctx.state.first_blood {
        gold += ctx.params.first_blood_gold;
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
        ctx.add_resource(assister.owner, "gold", ctx.params.assist_gold / heroes.len());
    }
}

fn share_xp(ctx, unit) {
    let heroes = ctx.find(unit, unit.position, ctx.params.xp_radius, "enemies:avatar");
    if heroes.is_empty() {
        return;
    }
    let xp = if unit.is_avatar {
        ctx.params.hero_xp_base + ctx.params.hero_xp_per_level * unit.level
    } else {
        unit.params.xp
    };
    for hero in heroes {
        ctx.add_xp(hero, "level", num(xp) / heroes.len());
    }
}
"#;
/// The test mode's `data/mode.toml`: its own keys, its content's modifiers, and the sections of
/// the modules it declares. Every stat its units carry, the 3v3's that `calc_damage` reads among
/// them; its pools, health its life, each of the stat of its name; its players' resources, its
/// layers, its slot kinds, `basic`, of 2 ranks, and `spell`, learned from the spawn; and its
/// tracks, `level`, its units' level, of levels 2 at 100 and 3 at 300, and `valor`, of level 2
/// at 50. Its one modifier, `blessing`, lasts 200 ms and stacks up to 4, with a count in its
/// state; `drill`, an upgrade a player holds for its grunts, has no end; `rally` shields by the
/// `ward` of the action that applies it, which no action of the mode gives.
const MODE_DATA: &str = r#"
name = "mode"
timers = ["a", "b", "each", "every", "late", "later", "once"]
resources = ["gold", "gems"]

[combat]
damage_kinds = ["physical", "magic", "true"]
life = "health"

[stats]
health = {}
mana = {}
move_speed = {}
armor = {}
armor_pen = {}
armor_pen_pct = {}
crit_chance = {}
damage_dealt_pct = {}
magic_pen = {}
magic_pen_pct = {}
magic_resist = {}
physical_block = {}

[pools.health]
max = "health"

[pools.mana]
max = "mana"

[navigation]
layers = ["ground", "air"]

[[slots]]
name = "basic"
ranks = 2

[[slots]]
name = "spell"

[tracks.level]
level = true
levels = [100, 300]

[tracks.valor]
levels = [50]

[tags.grunt]

[tags.tower]

[tags.crate]

[choices.duo]
name = "duo"
offers = "avatars"
unique = false
count = 2

[choices.faction]
name = "faction"
offers = "values"
unique = false
count = 1
values = ["america", "china"]

[choices.hero]
name = "hero"
offers = "avatars"
unique = true
count = 1

[choices.spells]
name = "spells"
offers = "loadout"
unique = false
count = 1
slot = "spell"

[inputs]
fail = "string"
gold = "string"
hero = "string"
markers = "string"
out = "string"
phase = "string"
probe = "string"
rich = "string"
spells = "string_list"

[state]
count = { type = "int", audience = "all" }
enemy = { type = "string", audience = "all" }
grunts = { type = "int", audience = "all" }
heroes = { type = "int", audience = "all" }
inputs = { type = "int", audience = "all" }
kind = { type = "string", audience = "none" }
neutral = { type = "string", audience = "all" }
owner = { type = "int", audience = "all" }
path = { type = "string", audience = "all" }
phase = { type = "string", default = "pick", audience = "all" }
players = { type = "int", audience = "all" }
seen = { type = "int", audience = "none" }
team = { type = "string", audience = "all" }
teams = { type = "int", audience = "all" }
tower_path = { type = "string", audience = "all" }

[player_state]
kills = { type = "int", audience = "owner" }
note = { type = "string", audience = "none" }
title = { type = "enum", values = ["rookie", "veteran"], audience = "all" }

[params]
gold = 8
group = ["grunt", "grunt"]
hero_xp_base = 150
hero_xp_per_level = 25
respawn_base_ms = 1000
respawn_per_level_ms = 500
xp_radius = 16

[options.bounty]
values = [3, 40]
default = 40

[[relations]]
teams = ["a", "neutral"]
relation = "neutral"
vision = true

[modifiers.blessing]
duration_ms = 200
reapply = "stack"
max_stacks = 4
state = { count = { type = "int" } }

[modifiers.drill]
reapply = "refresh"
affects = "allies:grunt"

[modifiers.rally]
shield = { param = "ward" }
"#;

/// The test mode's unit types, `data/units.toml`, each of 10 health, the walkers a meter a tick
/// at its 10 ticks a second: its grunt, kept when it dies, of its script state `hits` of 2,
/// which only its owner receives, and `mark`, a text of none, which every client that sees it
/// receives; its tower, a circle of 1 m on the second layer, which despawns when it dies; its
/// crate, a box of 2 by 2 m; neither of these walks. Its bolt is a type the mode does not spawn,
/// as a projectile's is: the view knows it, a spawn does not.
const UNITS: &str = r#"
[units.grunt]
tags = ["grunt"]
pools = ["health"]
stats = { health = { base = 10 }, move_speed = { base = 10 } }
combat = { on_death = "stay" }
state = { hits = { type = "int", default = 2, audience = "owner" }, mark = { type = "string", audience = "all" } }

[units.tower]
tags = ["tower"]
pools = ["health"]
stats = { health = { base = 10 } }
combat = {}
collision = { radius = "1.0", layer = "air" }

[units.crate]
tags = ["crate"]
pools = ["health"]
stats = { health = { base = 10 } }
combat = { on_death = "stay" }
collision = { box = ["2.0", "2.0"] }

[units.bolt]
projectile = { speed = "10" }
"#;

/// Bounds from (−10, −5) to (10, 6) with a grid of 1 m cells; one path, `mid`, along x; team a's
/// spawn at z = −5 and b's at 5; a's tower 8 m down the path; and a camp of a grunt in the middle.
const MAP: &str = r#"
[bounds]
min = [-10, -5]
max = [10, 6]

[grid]
cell = "1.0"

[navigation]
cell = "1.0"

[[paths]]
name = "mid"
points = [[-10, 0], [0, 0], [10, 0]]
walk = "once"

[[units]]
unit_type = "tower"
team = "a"
path = "mid"
position = [-8, 0]

[[markers]]
name = "a_spawn"
tags = ["spawn"]
team = "a"
position = [0, -5]

[[markers]]
name = "b_spawn"
tags = ["spawn"]
team = "b"
position = [0, 5]

[[markers]]
name = "camp"
tags = ["camp"]
position = [0, 0]
params = { unit_type = "grunt" }
"#;

/// Hero X, on both tracks, with one ability, `strike`, a cast of no target, cost, time or script,
/// of the 2 ranks of its slot kind.
const HERO_X: &str = r#"
name = "hero-x"
pools = ["health"]
stats = { health = { base = 10 }, move_speed = { base = 10 } }
combat = { on_death = "stay" }
tracks = ["level", "valor"]
slots = { basic = ["strike"] }

[actions.strike]
targeting = "none"
"#;

/// Hero Y, on both tracks, with its own `blessing` as its passive, which lasts 200 ms and stacks
/// up to 4, with a count in its state.
const HERO_Y: &str = r#"
name = "hero-y"
pools = ["health"]
stats = { health = { base = 10 }, move_speed = { base = 10 } }
combat = { on_death = "stay" }
tracks = ["level", "valor"]
passive = "blessing"

[modifiers.blessing]
duration_ms = 200
reapply = "stack"
max_stacks = 4
state = { count = { type = "int" } }
"#;

/// The mode's one spell, `blink`, a cast of no target, cost, time or script.
const SPELL: &str = r#"
[actions.blink]
targeting = "none"
"#;

/// The resource `name` of the mode's `RESOURCES`.
fn resource(name: &str) -> Option<ResourceId> {
    RESOURCES
        .into_iter()
        .collect::<Names<ResourceId>>()
        .named(name)
}

/// `text` with its one `old` replaced by `new`, as a test's change of a file of the test mode.
fn replaced(text: &str, old: &str, new: &str) -> String {
    assert_eq!(text.matches(old).count(), 1, "{old}");
    text.replace(old, new)
}

/// The test map's text made spatial, each point of its paths, its units and its markers up at
/// `y`.
fn raised(map: &str, y: i64) -> String {
    let mut raised = String::from("metric = \"spatial\"\n");
    for line in map.lines() {
        if line.starts_with("points = ") || line.starts_with("position = ") {
            let mut out = String::new();
            let mut rest = line;
            while let Some(open) = rest.find('[') {
                let (before, after) = rest.split_at(open);
                out.push_str(before);
                let close = after.find(']').unwrap();
                let pair = &after[1..close];
                if let Some((x, z)) = pair.split_once(", ")
                    && !x.contains('[')
                {
                    write!(out, "[{x}, {y}, {z}]").unwrap();
                    rest = &after[close + 1..];
                } else {
                    out.push('[');
                    rest = &after[1..];
                }
            }
            out.push_str(rest);
            raised.push_str(&out);
        } else {
            raised.push_str(line);
        }
        raised.push('\n');
    }
    raised
}

/// The text of the test mode's unit type `name`, a grunt of 1000 health and 100 mana and of
/// `stats`, each stat's base, which a test adds to its units.
fn stat_type(name: &str, stats: &[(&str, &str)]) -> String {
    let mut bases = String::new();
    for (stat, base) in stats {
        write!(bases, ", {stat} = {{ base = {base} }}").unwrap();
    }
    format!(
        "\n[units.{name}]\ntags = [\"grunt\"]\npools = [\"health\", \"mana\"]\n\
         stats = {{ health = {{ base = 1000 }}, mana = {{ base = 100 }}{bases} }}\n\
         combat = {{ on_death = \"stay\" }}\n"
    )
}

/// A unit of `team` of the test mode's unit type `name`, at the origin.
fn spawn_of(game: &mut Match, name: &str, team: Team) -> StableId {
    Spawn::named(game, name, Vec3::ZERO).team(team).place(game)
}

/// The test mode's teams: two player slots of a, one of b, and the neutral team's none.
fn teams() -> Vec<TeamManifest> {
    [("a", 2), ("b", 1), ("neutral", 0)]
        .map(|(name, player_slots)| TeamManifest {
            name: DeclaredName::new(name).unwrap(),
            player_slots,
        })
        .into()
}

/// The test mode's packages, its list holding `scripts`, the one at `own` its own, which alone
/// takes the picking helpers.
fn packages(scripts: &[&str], own: usize) -> PackageSet {
    let text = |name: &str, data: &str| DependencyText {
        name: name.to_owned(),
        data: data.to_owned(),
    };
    let scripts = scripts.iter().enumerate().map(|(at, script)| {
        if at == own {
            format!("{script}{PICK}")
        } else {
            (*script).to_owned()
        }
    });
    PackageSet {
        data: MODE_DATA.to_owned(),
        units: UNITS.to_owned(),
        map: MAP.to_owned(),
        avatars: vec![text("hero-x", HERO_X), text("hero-y", HERO_Y)],
        loadouts: vec![text("spells", SPELL)],
        scripts: scripts.collect(),
        own,
        teams: teams(),
        capabilities: vec![
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Vision,
            Capability::Abilities,
            Capability::Progression,
            Capability::Production,
        ],
        rate: RATE,
    }
}

/// The test mode's packages of its one script, `script`.
fn mode_packages(script: &str) -> PackageSet {
    packages(&[script], 0)
}

/// The test map, as the load reads it.
fn map() -> MapData {
    mode_packages("").map_data()
}

/// The match of `set` for 3 players, two on team `a` and one on `b`, with `limits`, before its
/// start, of what the host picked, `picks`.
fn installed_picked(set: &PackageSet, limits: ScriptLimits, picks: HostPicks) -> Match {
    let start = PackageStart {
        players: 3,
        limits,
        picks,
    };
    Match::of_package(set, start)
}

/// The match of `set` `installed_picked` gives, of the defaults of its options and no player slot's
/// pick.
fn installed(set: &PackageSet, limits: ScriptLimits) -> Match {
    let picks = HostPicks::defaults(&set.mode_data(), 3);
    installed_picked(set, limits, picks)
}

/// The match of `set` `installed` gives, after its start, every player slot held; an error when the
/// mode's start fails.
fn started(set: &PackageSet, limits: ScriptLimits) -> Result<Match, CallError> {
    let mut game = installed(set, limits);
    Mode::start_held(&mut game)?;
    Ok(game)
}

/// A match of the test mode of `script` for 3 players, two on team `a` and one on `b`, with
/// `limits`.
fn mode_match(script: &str, limits: ScriptLimits) -> Match {
    started(&mode_packages(script), limits).unwrap()
}

/// A match of `script`, whose `take_input` takes every mode input but a pick of a hero.
fn picking_match(script: &str, limits: ScriptLimits) -> Match {
    mode_match(&format!("{script}{PICKING}"), limits)
}

/// The match of the test mode whose list holds `scripts`, the one at `mode` its own, for 3
/// players, two on team `a` and one on `b`, with `limits`, after its start.
fn listed_match(scripts: &[&str], mode: usize, limits: ScriptLimits) -> Result<Match, CallError> {
    started(&packages(scripts, mode), limits)
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
mod player_state;
mod production;
mod progression;
mod restore;
mod roles;
mod rules_scripts;
mod start;
mod timers;
mod unit_state;
