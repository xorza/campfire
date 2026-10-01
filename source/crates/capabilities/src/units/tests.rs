use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_script::Budget;
use campfire_script::rhai::Dynamic;
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, Position, SimTick, StableId, Tick, Ticks,
};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combatant::Combatant;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::navigation::on_path::OnPath;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::level::Level;
use crate::stats::stat::{EngineStat, Stat};
use crate::stats::unit_state::UnitState;
use crate::stats::unit_stats::UnitStats;
use crate::units::path_id::PathId;
use crate::units::unit::Unit;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

fn unit() -> Combatant {
    Combatant {
        health: Health::new(num(10)).unwrap(),
        attack: Some(AttackStats::new(num(2), Ticks::new(0), Ticks::new(1), Num::ZERO).unwrap()),
        on_death: OnDeath::Stay,
    }
}

#[derive(Debug)]
struct Scene {
    world: World,
}

impl Scene {
    fn new() -> Scene {
        let limits = ScriptLimits {
            per_call: 10_000,
            player: 100_000,
            think: 100_000,
            mode: 100_000,
        };
        let scripts = MatchScripts {
            limits,
            players: 1,
            damage_kinds: Rc::from([]),
        };
        let TestMatch {
            world,
            schedule: _,
            registry: _,
        } = TestMatch::new(
            &[Capability::Stats, Capability::Combat],
            RATE,
            Some(scripts),
        );
        Scene { world }
    }

    fn unit_type(&mut self, tags: &[&str], params: &[(&str, Scalar)]) -> UnitType {
        let data = UnitTypeData {
            tags: tags.iter().map(|&tag| tag.to_owned()).collect(),
            params: params
                .iter()
                .map(|&(name, value)| (name.to_owned(), value))
                .collect::<BTreeMap<_, _>>(),
        };
        let name = format!("type {}", self.world.non_send::<View>().types_count());
        Units::load_type(&mut self.world, &name, &data).unwrap()
    }

    fn spawn(&mut self, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world.spawn((id, at, parts));
        id
    }

    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    /// `probe(ctx, of)` in `source`, run on the units as they are now.
    fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError> {
        let ctx = self.world.non_send::<Ctx>().clone();
        ctx.view().read(&self.world);
        let unit = ctx.view().unit(of).unwrap();
        let mut host = self.world.non_send_mut::<ScriptHost>();
        let script = host.compile(source).unwrap();
        let mut budget = Budget::new(u64::MAX);
        host.call(&mut budget, script, "probe", (ctx, unit))
            .map_err(CallError::from_script)
    }

    /// The stable ids of `value`, a unit or a list of units.
    fn ids(value: Dynamic) -> Vec<StableId> {
        let units = match value.clone().try_cast::<Vec<Dynamic>>() {
            Some(units) => units,
            None if value.is_unit() => Vec::new(),
            None => vec![value],
        };
        units
            .into_iter()
            .map(|unit| unit.try_cast::<Unit>().unwrap().id)
            .collect()
    }
}

#[test]
fn queries_select_living_units_by_filter_and_exact_ground_distance() {
    let mut scene = Scene::new();
    let creep = scene.unit_type(&["creep"], &[]);
    let of = scene.spawn(at(0, 0, 0), unit().bundle(Team::new(1)));
    // Up at y = 9, 3 m away on the ground plane: the nearest.
    let high = scene.spawn(at(0, 9, 3), unit().bundle(Team::new(0)));
    let east = scene.spawn(at(4, 0, 0), (creep, unit().bundle(Team::new(0))));
    let west = scene.spawn(at(-4, 0, 0), unit().bundle(Team::new(0)));
    let edge = scene.spawn(at(0, 0, 5), unit().bundle(Team::new(0)));
    let far = scene.spawn(at(6, 0, 0), unit().bundle(Team::new(0)));
    let ally = scene.spawn(at(1, 0, 0), unit().bundle(Team::new(1)));
    let dead = scene.spawn(at(0, 0, 1), (unit().bundle(Team::new(0)), Dead));
    // Within 2 m, but no target: one untargetable, one invulnerable.
    let hidden = UnitStats::in_states(&[UnitState::Untargetable]);
    let hidden = scene.spawn(at(2, 0, 0), (unit().bundle(Team::new(0)), hidden));
    let guarded = UnitStats::in_states(&[UnitState::Invulnerable]);
    let guarded = scene.spawn(at(-2, 0, 0), (unit().bundle(Team::new(0)), guarded));

    let find = |scene: &mut Scene, filter: &str| {
        let source = format!(r#"fn probe(ctx, of) {{ ctx.find(of, of.pos, 5, "{filter}") }}"#);
        Scene::ids(scene.probe(&source, of).unwrap())
    };
    // By stable id; the edge at exactly 5 m is in; the far one, the dead one and the two that are
    // no target are not.
    assert_eq!(find(&mut scene, "enemies"), [high, east, west, edge]);
    assert_eq!(find(&mut scene, "enemies:creep"), [east]);
    assert_eq!(find(&mut scene, "allies"), [of, ally]);
    assert_eq!(find(&mut scene, "all"), [of, high, east, west, edge, ally]);
    assert!(far.get() > 0 && dead.get() > 0 && hidden.get() > 0 && guarded.get() > 0);

    // The nearest in turn as each despawns: east and west tie at 4 m, and east has the lower id.
    let nearest = r#"fn probe(ctx, of) { ctx.nearest_visible(of, num(5), "enemies") }"#;
    let mut order = Vec::new();
    while let [next] = Scene::ids(scene.probe(nearest, of).unwrap())[..] {
        order.push(next);
        let entity = scene.entity(next);
        scene.world.despawn(entity);
    }
    assert_eq!(order, [high, east, west, edge]);

    let refusals = [
        (
            r#"ctx.find(of, of.pos, 5, "friends")"#,
            ApiError::UnknownFilter,
        ),
        (
            r#"ctx.find(of, of.pos, 5, "enemies:boss")"#,
            ApiError::UnknownTag,
        ),
        (
            r#"ctx.find(of, of.pos, -1, "all")"#,
            ApiError::NegativeRadius,
        ),
        (
            r#"ctx.nearest_visible(of, 1 << 40, "all")"#,
            ApiError::IntegerBeyondNum,
        ),
    ];
    for (call, refusal) in refusals {
        let source = format!("fn probe(ctx, of) {{ {call} }}");
        let error = scene.probe(&source, of).unwrap_err();
        assert!(
            matches!(error, CallError::Api(api) if api == refusal),
            "{call}: {error:?}"
        );
    }
}

#[test]
fn a_handle_reads_its_units_fields_as_the_view_read_them() {
    let mut scene = Scene::new();
    let window = ("help_window_ms", Scalar::Int(2000));
    let range = ("aggro_range", Scalar::Decimal(num(7)));
    let hero = scene.unit_type(&["avatar"], &[window, range]);
    // On a path, but the scene has no `navigation` to fill `unit.path`.
    let owner = Owner::new(PlayerSlot::new(2));
    let path = OnPath::new(PathId::new(0));
    let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
    let of = scene.spawn(
        at(0, 0, 0),
        (hero, unit().bundle(Team::new(0)), owner, path, body),
    );
    let near = scene.spawn(at(3, 0, 4), unit().bundle(Team::new(1)));
    let recent = scene.spawn(at(9, 0, 0), unit().bundle(Team::new(1)));
    let fallen = scene.spawn(at(9, 0, 1), (unit().bundle(Team::new(1)), Dead));
    let health_only = (Team::new(1), Health::new(num(1)).unwrap());
    let bare = scene.spawn(at(9, 0, 2), health_only);
    // 3 m away on the ground plane, √153 ≈ 12.37 m in space.
    scene.spawn(at(0, 12, 3), unit().bundle(Team::new(1)));
    let entity = scene.entity(of);
    scene
        .world
        .get_mut::<AttackState>(entity)
        .unwrap()
        .set_target(Some(near));

    // At 30 ticks a second, 2000 ms is 60 ticks: in tick 100, a strike in tick 40 is recent, and
    // one in tick 39 is not; a dead attacker is never returned.
    scene.world.insert_resource(SimTick::new(Tick::new(100)));
    let index = scene.world.resource::<EntityIndex>();
    let mut attackers = RecentAttackers::default();
    // A strike later than the view's tick, as a rollback can leave, is not recent either.
    for (source, tick) in [(near, 39), (recent, 40), (fallen, 100), (bare, 101)] {
        attackers.record(source, Tick::new(tick), index);
    }
    *scene.world.get_mut::<RecentAttackers>(entity).unwrap() = attackers;

    let read = |scene: &mut Scene, expression: &str| {
        let source = format!("fn probe(ctx, of) {{ {expression} }}");
        scene.probe(&source, of)
    };
    let value = |scene: &mut Scene, expression: &str| read(scene, expression).unwrap();
    assert!(value(&mut scene, "of.is_avatar").as_bool().unwrap());
    assert!(value(&mut scene, "of.alive").as_bool().unwrap());
    assert_eq!(value(&mut scene, "of.owner").as_int(), Ok(2));
    assert!(value(&mut scene, "of.path").is_unit());
    // Its body's radius, 0.75 m; a unit with no body has none.
    let radius = |scene: &mut Scene, unit| {
        let source = "fn probe(ctx, of) { of.radius }";
        scene.probe(source, unit).unwrap().cast::<Num>()
    };
    assert_eq!(radius(&mut scene, of), body.radius());
    assert_eq!(radius(&mut scene, near), Num::ZERO);
    assert_eq!(
        value(&mut scene, "of.params.help_window_ms").as_int(),
        Ok(2000)
    );
    assert_eq!(
        value(&mut scene, "of.params.aggro_range").cast::<Num>(),
        num(7)
    );
    assert_eq!(value(&mut scene, "of.attack_range").cast::<Num>(), num(2));
    // 3, 4, 5: exact.
    let distance = "of.pos.distance_to(of.target.pos)";
    assert_eq!(value(&mut scene, distance).cast::<Num>(), num(5));
    assert!(
        value(&mut scene, "of.target.is_enemy_of(of)")
            .as_bool()
            .unwrap()
    );
    assert!(!value(&mut scene, "of.target.is_avatar").as_bool().unwrap());
    assert!(
        value(&mut scene, "of.target != () && of != ()")
            .as_bool()
            .unwrap()
    );
    // Exactly, on the ground plane, as every range.
    let reach = "of.pos.within(of.target.pos, 5) && !of.pos.within(of.target.pos, 4)";
    assert!(value(&mut scene, reach).as_bool().unwrap());
    let above = r#"let up = ctx.find(of, of.pos, 3, "enemies")[0];
        of.pos.within(up.pos, 3) && !of.pos.within(up.pos, 2) && of.pos.distance_to(up.pos) > 12"#;
    assert!(value(&mut scene, above).as_bool().unwrap());
    let attackers = value(&mut scene, "of.recent_attackers(2000)");
    assert_eq!(Scene::ids(attackers), [recent]);
    // 2034 ms is 61.02 ticks, up to 62: tick 39 is in.
    let attackers = value(&mut scene, "of.recent_attackers(2034)");
    assert_eq!(Scene::ids(attackers), [near, recent]);

    let refusals = [
        ("of.params.gold", ApiError::UnknownParam),
        ("of.recent_attackers(-1)", ApiError::NegativeTime),
        ("of.pos.within(of.pos, -1)", ApiError::NegativeRadius),
    ];
    for (expression, refusal) in refusals {
        let error = read(&mut scene, expression).unwrap_err();
        assert!(
            matches!(error, CallError::Api(api) if api == refusal),
            "{expression}"
        );
    }
    let error = scene
        .probe("fn probe(ctx, of) { of.attack_range }", bare)
        .unwrap_err();
    assert!(
        matches!(error, CallError::Api(ApiError::NoAttack)),
        "{error:?}"
    );

    // A target the view did not read is `()`.
    let entity = scene.entity(near);
    scene.world.despawn(entity);
    assert!(value(&mut scene, "of.target == ()").as_bool().unwrap());
}

#[test]
fn a_handle_reads_its_units_level_health_and_stats() {
    let mut scene = Scene::new();
    // Level 3, with health 10 and armor 25 among the mode's stats, health first as an engine
    // stat; 4 health taken. One unit has health and no stats, one neither.
    let names = [
        Stat::Engine(EngineStat::Health),
        Stat::Declared(DeclaredName::new("armor").unwrap()),
    ];
    scene
        .world
        .non_send::<View>()
        .set_stat_names(Rc::from(names));
    let mut stats = UnitStats::default();
    stats.refill().extend([num(10), num(25)]);
    let of = scene.spawn(
        at(0, 0, 0),
        (unit().bundle(Team::new(0)), Level::new(3).unwrap(), stats),
    );
    let entity = scene.entity(of);
    scene.world.get_mut::<Health>(entity).unwrap().take(num(4));
    let bare = scene.spawn(at(1, 0, 0), (Team::new(1), Health::new(num(1)).unwrap()));
    let shell = scene.spawn(at(2, 0, 0), Team::new(1));

    let read = |scene: &mut Scene, unit, expression: &str| {
        let source = format!("fn probe(ctx, of) {{ {expression} }}");
        scene.probe(&source, unit)
    };
    let value = |scene: &mut Scene, expression: &str| read(scene, of, expression).unwrap();
    assert_eq!(value(&mut scene, "of.level").as_int(), Ok(3));
    assert_eq!(value(&mut scene, "of.health").cast::<Num>(), num(6));
    assert_eq!(value(&mut scene, "of.max_health").cast::<Num>(), num(10));
    assert_eq!(
        value(&mut scene, r#"of.stat("armor")"#).cast::<Num>(),
        num(25)
    );
    assert_eq!(
        value(&mut scene, r#"of.stat("health")"#).cast::<Num>(),
        num(10)
    );

    let refusals = [
        (of, r#"of.stat("spirit")"#, ApiError::UnknownStat),
        (bare, "of.level", ApiError::NoStats),
        (bare, r#"of.stat("armor")"#, ApiError::NoStats),
        (shell, "of.health", ApiError::NoHealth),
        (shell, "of.max_health", ApiError::NoHealth),
    ];
    for (unit, expression, refusal) in refusals {
        let error = read(&mut scene, unit, expression).unwrap_err();
        assert!(
            matches!(error, CallError::Api(api) if api == refusal),
            "{expression}: {error:?}"
        );
    }
}

#[test]
fn a_unit_type_name_is_one_types_only() {
    let mut scene = Scene::new();
    let data = UnitTypeData::default();
    let first = Units::load_type(&mut scene.world, "grunt", &data).unwrap();
    assert_eq!(
        Units::load_type(&mut scene.world, "grunt", &data),
        Err(UnitTypeError::RepeatedName)
    );
    let second = Units::load_type(&mut scene.world, "tower", &data).unwrap();
    let view = scene.world.non_send::<View>();
    assert_eq!(
        (view.unit_type("grunt"), view.unit_type("tower")),
        (Some(first), Some(second))
    );
    assert_eq!(view.unit_type("wolf"), None);
}
