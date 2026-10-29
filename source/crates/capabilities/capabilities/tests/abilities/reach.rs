use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use campfire_abilities::*;
use campfire_actions::{
    ActionData, ActionKeys, ActionProgress, ActionRange, ActionTarget, InProgress, OrderPhase,
    RangeField, SlotAim, Targeting,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Order};
use campfire_common::PlayerSlot;
use campfire_geometry::BodyBox;
use campfire_math::{Num, Vec3};
use campfire_navigation::Navigation;
use campfire_orders::OrdersDirective;
use campfire_script::ScriptLimits;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimTick, SimWorld};
use campfire_stats::Stats;
use campfire_units::internals::Spawn;
use campfire_units::{Body, Dead, Destination, GroundPoint, MoveStep, Units};
use campfire_values::{ActionId, Block, FilterData, Number, PackagePath, Ranked, Scalar, Team};

use crate::abilities::{abilities_match, half_meters};

#[test]
fn a_point_aim_beyond_the_range_clamps_in_to_it_when_the_action_says_so() {
    // 10 true damage to each enemy within half a meter of the point the cast resolves at.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, target, num(1) / 2, "enemies") {
        ctx.damage(unit, "true", 10);
    }
}
"#;
    let leap = |clamp| ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("leap.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(4),
            )))),
            clamp_to_range: clamp,
            ..ActionData::of(Targeting::Point, CastData::default()).keys
        },
        ..ActionData::of(Targeting::Point, CastData::default())
    };
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let clamped = Abilities::load(&mut game, "leap", &leap(true), script);
    let plain = Abilities::load(&mut game, "hop", &leap(false), script);
    let point = |x, z| ActionTarget::Point(Vec3::from((x, 0, z)));
    let enemy = |game: &mut Match, x, z| {
        Spawn::named(game, "combatant", (x, 0, z))
            .team(Team::new(1))
            .place(game)
    };
    // From the origin, 4 m of range: an aim at 10 m along x lands at 4 m; one at 2 m stays.
    for (aim, at) in [(point(10, 0), (4, 0)), (point(2, 0), (2, 0))] {
        let caster = Abilities::caster(&mut game, clamped, 1);
        let struck = enemy(&mut game, at.0, at.1);
        Abilities::cast(&mut game, caster, aim);
        assert_eq!(Stats::health(&game, struck), 490, "{at:?}");
    }
    // An aim at (9, 12), 15 m off, lands on its line at 4 m: (2.4, 3.2), rounded once each, and
    // a bit shorter where that rounding ends past the range; the enemy there is struck.
    let caster = Abilities::caster(&mut game, clamped, 1);
    let struck = Spawn::named(
        &game,
        "combatant",
        Vec3::from((Num::int(12) / 5, 0, Num::int(16) / 5)),
    )
    .team(Team::new(1))
    .place(&mut game);
    Abilities::cast(&mut game, caster, point(9, 12));
    assert_eq!(Stats::health(&game, struck), 490);
    // An action that does not clamp starts no cast at a point beyond its range.
    let caster = Abilities::caster(&mut game, plain, 1);
    let missed = enemy(&mut game, 4, 0);
    Abilities::cast(&mut game, caster, point(10, 0));
    assert_eq!(Stats::health(&game, missed), 500);
    // A caster with a box of 4 by 2 m at the origin reaches 4 m from the box's edge: an aim at
    // (10, 0) lands at 6 m, 4 m past its edge at x = 2, exactly, and strikes an enemy at 5.6 m.
    // One at (9, 12) leaves the box through its top, then nears its corner (2, 1): it lands where
    // the corner is 4 m off, t = 2 + √15 ≈ 5.873 m along, at about (3.524, 4.698), and strikes an
    // enemy 0.3 m short of it, at (3.34, 4.46). Measured from the box's bound, 2.24 m round its
    // center, each would land 4 + 2.24 m along, 0.24 m and 0.36 m further, past both enemies'
    // half meter.
    let boxed = |game: &mut Match| {
        let caster = Abilities::caster(game, clamped, 1);
        let body = BodyBox::new([Num::int(4), Num::int(2)], Num::ZERO).unwrap();
        game.insert_parts(caster, Body::of_box(body));
        caster
    };
    let near =
        |x: &str, z: &str| Vec3::from((x.parse::<Num>().unwrap(), 0, z.parse::<Num>().unwrap()));
    let caster = boxed(&mut game);
    let struck = Spawn::named(&game, "combatant", near("5.6", "0"))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(&mut game, caster, point(10, 0));
    assert_eq!(Stats::health(&game, struck), 490);
    let caster = boxed(&mut game);
    let struck = Spawn::named(&game, "combatant", near("3.34", "4.46"))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(&mut game, caster, point(9, 12));
    assert_eq!(Stats::health(&game, struck), 490);
    assert_eq!(Abilities::failed_calls(&mut game), []);
}

/// Hop, which deals 10 to each enemy within half a meter of its point, and Jab, which deals 10 to
/// its target, in a match of units that walk and take orders; each reaches 4 m, and resolves as
/// it starts; and Lob, a jab that takes no target nearer than 2 m.
#[derive(Debug, Clone, Copy)]
struct Reaching {
    hop: ActionId,
    jab: ActionId,
    lob: ActionId,
}

impl Reaching {
    /// The capabilities a reaching match declares.
    const CAPABILITIES: [Capability; 5] = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];

    /// Loads Hop and Jab into `game`, a match of `CAPABILITIES`.
    fn place(game: &mut Match) -> Self {
        let hop = ActionData {
            keys: ActionKeys {
                script: Some(PackagePath::parse("hop.rhai").unwrap()),
                range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                    Num::int(4),
                )))),
                ..ActionData::of(Targeting::Point, CastData::default()).keys
            },
            ..ActionData::of(Targeting::Point, CastData::default())
        };
        let jab = ActionData {
            keys: ActionKeys {
                script: Some(PackagePath::parse("jab.rhai").unwrap()),
                targeting: Targeting::Unit(FilterData::parse("enemies").unwrap()),
                ..hop.clone().keys
            },
            ..hop.clone()
        };
        let hop_script = r#"
    fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, target, num(1) / 2, "enemies") {
        ctx.damage(unit, "true", 10);
    }
    }
    "#;
        let jab_script =
            r#"fn on_resolve(ctx, caster, target) { ctx.damage(target, "true", 10); }"#;
        let lob = ActionData {
            keys: ActionKeys {
                min_range: Some(Ranked::One(Number::Value(Scalar::Int(2)))),
                ..jab.clone().keys
            },
            ..jab.clone()
        };
        let hop = Abilities::load(game, "hop", &hop, hop_script);
        let lob = Abilities::load(game, "lob", &lob, jab_script);
        let jab = Abilities::load(game, "jab", &jab, jab_script);
        Self { hop, jab, lob }
    }
}

/// A unit that walks `meters` a tick.
fn walker(meters: Num) -> impl Bundle<Effect: NoBundleEffect> {
    Navigation::walker(MoveStep::new(meters).unwrap())
}

#[test]
fn a_cast_beyond_its_range_walks_in_range_first_and_starts_there() {
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &Reaching::CAPABILITIES);
    let reaching = Reaching::place(&mut game);
    // A hop at (10, 0) from the origin, a meter a tick: the Act stage of tick t finds the caster
    // at t, before that tick's step, and 10 − t m within 4 m first in tick 6, where it stops and
    // hops.
    let hopper = Spawn::named(&game, "walking caster", Vec3::ZERO)
        .parts(Abilities::caster_parts(reaching.hop, 1))
        .place(&mut game);
    let still = Spawn::named(&game, "combatant", (10, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(
        &mut game,
        hopper,
        ActionTarget::Point(Vec3::from((10, 0, 0))),
    );
    assert!(game.part::<ActionProgress>(hopper).is_approaching());
    game.run_until(6);
    assert_eq!(
        (game.part::<Position>(hopper).x, Stats::health(&game, still)),
        (Num::int(6), 500)
    );
    game.step();
    assert_eq!(
        (game.part::<Position>(hopper).x, Stats::health(&game, still)),
        (Num::int(6), 490)
    );
    game.run_until(20);
    assert_eq!(game.part::<Position>(hopper).x, Num::int(6));

    // A jab at an enemy at 12 m that walks away half a meter a tick: the Act stage of tick t
    // finds it at 12 + t/2 and the caster at t, 12 − t/2 m apart, within 4 m first in tick 16.
    let jabber = Spawn::named(&game, "walking caster", Vec3::ZERO)
        .parts(Abilities::caster_parts(reaching.jab, 1))
        .place(&mut game);
    let start = game.resource::<SimTick>().start().get();
    let fleeing = Spawn::named(&game, "combatant", (12, 0, 0))
        .team(Team::new(1))
        .parts(walker(half_meters(1)))
        .place(&mut game);
    let away = Destination::to(Some(Vec3::from((100, 0, 0))));
    game.insert_parts(fleeing, away);
    Abilities::cast(&mut game, jabber, ActionTarget::Unit(fleeing));
    game.run_until(start + 16);
    assert_eq!(Stats::health(&game, fleeing), 500);
    game.step();
    assert_eq!(
        (
            game.part::<Position>(jabber).x,
            Stats::health(&game, fleeing)
        ),
        (Num::int(16), 490)
    );

    // A lob at an enemy 1 m off, inside its minimum of 2 m: the caster walks straight away from
    // it to halfway between 2 and 4 m, (−2, 0, 0), a meter a tick; in tick 1 it stands at −1, 2 m
    // off, at its minimum, which it takes, and stops there. A caster that does not walk drops it.
    let lobber = Spawn::named(&game, "walking caster", Vec3::ZERO)
        .parts(Abilities::caster_parts(reaching.lob, 1))
        .place(&mut game);
    let start = game.resource::<SimTick>().start().get();
    let close = Spawn::named(&game, "combatant", (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(&mut game, lobber, ActionTarget::Unit(close));
    assert!(game.part::<ActionProgress>(lobber).is_approaching());
    assert_eq!(
        game.part::<Destination>(lobber).get(),
        Some(Vec3::from((-2, 0, 0)))
    );
    game.run_until(start + 1);
    assert_eq!(Stats::health(&game, close), 500);
    game.step();
    assert_eq!(
        (game.part::<Position>(lobber).x, Stats::health(&game, close)),
        (Num::int(-1), 490)
    );
    assert_eq!(game.part::<Destination>(lobber).get(), None);
    let standing = Abilities::caster(&mut game, reaching.lob, 1);
    let beside = Spawn::named(&game, "combatant", (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(&mut game, standing, ActionTarget::Unit(beside));
    assert_eq!(game.part::<ActionProgress>(standing).in_progress(), None);
    game.step();
    assert_eq!(Stats::health(&game, beside), 500);
    assert_eq!(Abilities::failed_calls(&mut game), []);
}

#[test]
fn a_new_order_a_block_or_a_failed_check_ends_a_walk_in_range() {
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &Reaching::CAPABILITIES);
    let reaching = Reaching::place(&mut game);
    let mover = Spawn::named(&game, "walking caster", Vec3::ZERO)
        .parts(Abilities::caster_parts(reaching.jab, 1))
        .place(&mut game);
    let far = Spawn::named(&game, "combatant", (0, 0, 30))
        .team(Team::new(1))
        .place(&mut game);
    let send = |game: &mut Match, action| {
        game.step_orders(PlayerSlot::new(0), &[Order::one(mover, action)]);
    };
    let jab = OrdersDirective::Slot {
        slot: 0,
        target: ActionTarget::Unit(far),
    };
    let approaching = |game: &Match| game.part::<ActionProgress>(mover).is_approaching();
    // A move order ends the walk to jab, and the jab: the unit walks where it was told.
    send(&mut game, jab);
    assert!(approaching(&game));
    let back = Vec3::from((0, 0, -5));
    send(
        &mut game,
        OrdersDirective::Move {
            to: GroundPoint {
                x: back.x,
                z: back.z,
            },
        },
    );
    assert_eq!(game.part::<ActionProgress>(mover).in_progress(), None);
    assert_eq!(game.part::<Destination>(mover).get(), Some(back));
    // A silence keeps the jab as an order, and the unit stands until it ends; then it walks on.
    send(&mut game, jab);
    assert!(approaching(&game));
    Units::set_blocks(&mut game, mover, &[Block::Cast]);
    game.step();
    let ordered = InProgress::Order {
        aim: SlotAim {
            slot: 0,
            target: ActionTarget::Unit(far),
        },
        phase: OrderPhase::Ordered,
    };
    assert_eq!(
        game.part::<ActionProgress>(mover).in_progress(),
        Some(ordered)
    );
    assert_eq!(game.part::<Destination>(mover).get(), None);
    Units::set_blocks(&mut game, mover, &[]);
    game.step();
    assert!(approaching(&game));
    // A target that dies drops the jab, and stops the walk where it is.
    let entity = game.unit_entity(far);
    game.entity_mut(entity).insert(Dead);
    game.step();
    assert_eq!(game.part::<ActionProgress>(mover).in_progress(), None);
    assert_eq!(game.part::<Destination>(mover).get(), None);
    assert_eq!(Abilities::failed_calls(&mut game), []);
}
