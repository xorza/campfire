use std::fmt::Write as _;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionSlots, ActionTarget, Actions, SlotKind};
use campfire_capabilities::{Match, Release};
use campfire_combat::{
    Damage, DamageCause, DamagePass, Heal, HealCause, PassEntry, RecentAttackers, Respawn,
};
use campfire_common::{PlayerSlot, Ticks};
use campfire_geometry::{Grid, Metric};
use campfire_math::{Num, Vec3};
use campfire_mode::{
    InputValue, MatchResult, ModeError, ModeInput, Offer, UnansweredPlayerSlotEvents, *,
};
use campfire_navigation::{NavigationBook, OnPath, PathEnd, Route};
use campfire_players::{MatchEntity, OutOfPlay, Player, PlayerResources, PlayersWorld};
use campfire_production::{Queued, TrainQueue};
use campfire_progression::{Experience, Points, TrackSet};
use campfire_runner::MatchBuild;
use campfire_script::{
    ApiError, BudgetOf, CallError, FailureKind, ListPlace, MatchNames, ParamProblem, ScriptBudgets,
    ScriptFailures, ScriptLimits, ScriptRole, StateValue,
};
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, PlayerSlotEvent, PlayerSlotEventKind, SimComponent,
    SimResource, SimTick, StableId,
};
use campfire_stats::{
    Application, Hold, Level, ModifierClocks, Modifiers, NewInstance, PlayerModifiers, Pools,
    Reapply, StatBook, StatShare, Stats, UnitStats,
};
use campfire_test_packages::{Edit, PackageTree, TestMode};
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

/// The test mode's files a test edits, by their paths from its tree's root.
const MANIFEST: &str = "modes/hooks/manifest.toml";
const DATA: &str = "modes/hooks/data/mode.toml";
const UNITS: &str = "modes/hooks/data/units.toml";
const MAP: &str = "modes/hooks/map/arena/map.toml";
const LOCALE: &str = "modes/hooks/locale/en.ftl";
const HERO_X: &str = "heroes/x/data/avatar.toml";
const HERO_Y: &str = "heroes/y/data/avatar.toml";

/// The test mode's packages, its list holding `scripts`, the one at `own` its own, which alone
/// takes the picking helpers: `scripts/mode.rhai`, and each other `scripts/<place>.rhai`.
fn packages(scripts: &[&str], own: usize) -> PackageTree {
    let mut tree = PackageTree::of(TestMode::Hooks);
    let mut list = Vec::new();
    for (at, script) in scripts.iter().enumerate() {
        let (path, text) = if at == own {
            ("scripts/mode.rhai".to_owned(), format!("{script}{PICK}"))
        } else {
            (format!("scripts/{at}.rhai"), (*script).to_owned())
        };
        tree.edit(&format!("modes/hooks/{path}"), Edit::Create(&text));
        list.push(if at == own && scripts.len() > 1 {
            format!("{{ path = \"{path}\", mode = true }}")
        } else {
            format!("\"{path}\"")
        });
    }
    tree.edit(
        DATA,
        Edit::Set("scripts", &format!("[{}]", list.join(", "))),
    );
    tree
}

/// The test mode's packages of its one script, `script`.
fn mode_packages(script: &str) -> PackageTree {
    packages(&[script], 0)
}

/// The test map, as the load reads it.
fn map() -> MapData {
    TestMode::Hooks.packages().map().clone()
}

/// The match of `tree` for 3 players, two on team `a` and one on `b`, its scripts within
/// `limits`, before its start, of the defaults of its options and no player slot's pick.
fn installed(tree: &PackageTree, limits: ScriptLimits) -> Match {
    let ScriptLimits {
        per_call,
        player,
        think,
        mode,
    } = limits;
    let limits =
        format!("{{ per_call = {per_call}, player = {player}, think = {think}, mode = {mode} }}");
    let mut tree = tree.clone();
    tree.edit(MANIFEST, Edit::Set("script_limits", &limits));
    MatchBuild::test_target(&tree.packages(), 3)
}

/// The match of `tree` `installed` gives, after its start, every player slot held; an error when
/// the mode's start fails.
fn started(tree: &PackageTree, limits: ScriptLimits) -> Result<Match, CallError> {
    let mut game = installed(tree, limits);
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
