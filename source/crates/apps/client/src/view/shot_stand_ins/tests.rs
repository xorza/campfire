use std::f32::consts::FRAC_PI_2;
use std::num::NonZeroU32;

use bevy::app::{App, FixedPostUpdate};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::world::World;
use bevy::math::Quat;
use bevy::time::Time;
use campfire_actions::internals::TestAction;
use campfire_actions::{ActionKind, ActionRange, Actions, Aim, ShotDamage, SlotKind};
use campfire_declarations::ByIdBuilder;
use campfire_math::{Num, Vec3 as SimVec3};
use campfire_projectiles::{Flight, Payload, Projectile, ProjectileData};
use campfire_script::{MatchNames, TagNames};
use campfire_sim::{IdAllocator, MatchRate, Unpredicted};
use campfire_units::BodyUnit;
use campfire_values::{ActionId, DamageKind, Rank, Team};

use super::*;
use crate::test_app;
use crate::view::ViewPlugin;

/// A shooter's type, of a weapon of two barrels, and its projectile's, of no look.
const SHOOTER: UnitType = UnitType::new(0);
const SHELL: UnitType = UnitType::new(1);

/// A view whose match has a weapon that fires `SHELL` at 30 m/s, a meter a tick, from the
/// shooter's barrels at (1, 2, 0) and (2, 2, 0), each along `+x`; its weapon's id.
fn view() -> (App, ActionId) {
    let mut app = test_app::view();
    app.add_plugins(ViewPlugin);
    app.init_resource::<MatchNames>();
    let fire = TestAction::new(
        ActionKind::Attack,
        Aim::None,
        ActionRange::meters(Num::int(10)),
        Ticks::ZERO,
    );
    let weapon = Actions::add(app.world_mut(), "cannon", fire.homing(SHELL));
    let way = SimVec3::from((1, 0, 0));
    let barrels = Barrels::of_weapon(
        weapon,
        &[
            [SimVec3::from((1, 2, 0)), way],
            [SimVec3::from((2, 2, 0)), way],
        ],
    );
    let mut book = ByIdBuilder::default();
    book.set(SHOOTER, barrels);
    app.insert_resource(book.build());
    let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
    let data = ProjectileData::flying(Num::int(30));
    let spec = ProjectileSpec::of(&data, &TagNames::default(), rate).unwrap();
    let mut book = ByIdBuilder::default();
    book.set(SHELL, spec);
    app.insert_resource(book.build());
    app.update();
    (app, weapon)
}

/// Runs the ticks the client runs to `tick`, as its fixed loop steps the stand-ins.
fn tick(app: &mut App, tick: u64) {
    app.insert_resource(SimTick::new(Tick::new(tick)));
    app.world_mut().run_schedule(FixedPostUpdate);
}

/// The stand-ins' roots, oldest first.
fn roots(app: &App) -> Vec<Entity> {
    let stand_ins = app.world().resource::<ShotStandIns>();
    stand_ins.0.iter().map(|stand_in| stand_in.root).collect()
}

#[test]
fn a_predicted_shot_draws_a_stand_in_at_its_barrel_which_the_servers_projectile_takes_over() {
    let (mut app, weapon) = view();
    // The client holds the server's state of tick 8, behind the ticks it predicts.
    app.insert_resource(ReceivedTick::new(Some(Tick::new(8))));
    let mut ids = IdAllocator::default();
    let slots = ActionSlots::new([(weapon, SlotKind::new(0), Some(Rank::FIRST))]);
    // The client's own shooter at (10, 0, 5), facing 90°, which turns `+x` to `−z`; another
    // player's at the same place.
    let source = ids.allocate();
    let shooter = (
        BodyUnit,
        Position::from((10, 0, 5)),
        Team::new(0),
        SHOOTER,
        slots,
        Facing::of(Num::int(90)),
    );
    let own = app.world_mut().spawn((source, shooter.clone())).id();
    let other = ids.allocate();
    let received = app.world_mut().spawn((other, shooter, Unpredicted)).id();
    app.update();
    for unit in [own, received] {
        let root = app.world().get::<Drawing>(unit).unwrap().root();
        app.world_mut().entity_mut(root).insert(FireSeen::default());
    }
    // Each fires at tick 10. The own shot's stand-in stands at its first barrel, turned: (10, 0,
    // 5) + (0, 2, −1), along `−z`, its `+x` turned a quarter about the vertical; the received
    // unit's shot draws none, as the server's projectile comes as soon.
    app.insert_resource(SimTick::new(Tick::new(10)));
    for unit in [own, received] {
        app.world_mut().trigger(ShotSeen {
            unit,
            slot: 0,
            action: weapon,
        });
    }
    app.world_mut().flush();
    let [first] = roots(&app)[..] else {
        panic!("one stand-in");
    };
    let world = app.world();
    let first_at = Vec3::new(10.0, 2.0, 4.0);
    let placed = world.get::<Transform>(first).unwrap();
    assert_eq!(placed.translation, first_at);
    assert!(
        placed
            .rotation
            .abs_diff_eq(Quat::from_rotation_y(FRAC_PI_2), 1e-6),
        "{}",
        placed.rotation
    );
    assert_eq!(
        *world.get::<Glide>(first).unwrap(),
        Glide::ticked(Spot {
            point: first_at,
            yaw: 0.0
        })
    );
    // A type of no look stands in as the palette's ball.
    assert_eq!(world.get::<Children>(first).unwrap().len(), 1);
    // The next shot, at tick 11, leaves the second barrel, a meter further along `−z`.
    app.insert_resource(SimTick::new(Tick::new(11)));
    app.world_mut().trigger(ShotSeen {
        unit: own,
        slot: 0,
        action: weapon,
    });
    app.world_mut().flush();
    let [_, second] = roots(&app)[..] else {
        panic!("two stand-ins");
    };
    let second_at = Vec3::new(10.0, 2.0, 3.0);
    assert_eq!(
        app.world().get::<Transform>(second).unwrap().translation,
        second_at
    );

    // After ticks 12 and 13 the first has flown 3 m along `−z`, from z = 4 to 1, drawn a tick
    // late, at z = 2, at the start of the tick's share.
    tick(&mut app, 12);
    tick(&mut app, 13);
    let drawn = |z| Spot {
        point: Vec3::new(10.0, 2.0, z),
        yaw: 0.0,
    };
    assert_eq!(
        *app.world().get::<Glide>(first).unwrap(),
        Glide::Ticked {
            before: drawn(2.0),
            last: drawn(1.0),
        }
    );
    app.update();
    assert_eq!(
        app.world().get::<Transform>(first).unwrap().translation,
        drawn(2.0).point
    );

    // The server's state of tick 11 brings the projectile of the own shooter where it launched
    // from the second barrel: of the two stand-ins within 5 ticks, it takes the nearest tick's,
    // the second, and not the older first, whose shot the server did not fire. Its drawing
    // takes the second's place and turn, z = 2, and glides from there to its own place, and the
    // stand-in goes. The other unit's projectile takes none.
    let payload = Payload::Attack(ShotDamage {
        action: weapon,
        amount: Num::ONE,
        outer: None,
        kind: DamageKind::new(0),
        rank: Rank::FIRST,
        roll: Num::ZERO,
    });
    let projectile = |source| {
        let flight = Flight::Homing { target: other };
        Projectile::new(source, flight, payload).unwrap()
    };
    let received = |app: &mut App, tick| {
        app.insert_resource(ReceivedTick::new(Some(Tick::new(tick))));
        app.update();
    };
    let launched = Position::from((10, 2, 3));
    let taken = app
        .world_mut()
        .spawn((
            ids.allocate(),
            launched,
            SHELL,
            projectile(source),
            Unpredicted,
        ))
        .id();
    let others = app
        .world_mut()
        .spawn((
            ids.allocate(),
            launched,
            SHELL,
            projectile(other),
            Unpredicted,
        ))
        .id();
    received(&mut app, 11);
    assert_eq!(roots(&app), [first]);
    let world = app.world();
    assert!(world.get_entity(second).is_err());
    let root = world.get::<Drawing>(taken).unwrap().root();
    let now = world.resource::<Time>().elapsed();
    let Glide::Timed { from, to, since } = *world.get::<Glide>(root).unwrap() else {
        panic!("a received projectile's glide is timed");
    };
    assert_eq!((from.point, to, since), (drawn(2.0).point, drawn(3.0), now));
    let turned = world.get::<Transform>(root).unwrap().rotation;
    assert!(turned.abs_diff_eq(Quat::from_rotation_y(FRAC_PI_2), 1e-6));
    let apart = world.get::<Drawing>(others).unwrap().root();
    assert_eq!(
        *world.get::<Glide>(apart).unwrap(),
        Glide::timed(drawn(3.0), now)
    );
    assert_eq!(
        world.get::<Transform>(apart).unwrap().rotation,
        Quat::IDENTITY
    );

    // The first, which no projectile takes over, stands while the state of a tick within 5 of
    // its own, 10, may still bring its projectile, and ends with that of tick 16.
    received(&mut app, 15);
    assert_eq!(roots(&app), [first]);
    received(&mut app, 16);
    assert!(roots(&app).is_empty());
    assert!(app.world().get_entity(first).is_err());
    // A shot at tick 20 stands while the client holds the state of tick 17, and ends as the
    // client leaves its match and holds none.
    app.insert_resource(SimTick::new(Tick::new(20)));
    app.world_mut().trigger(ShotSeen {
        unit: own,
        slot: 0,
        action: weapon,
    });
    received(&mut app, 17);
    let [left] = roots(&app)[..] else {
        panic!("one stand-in");
    };
    app.insert_resource(ReceivedTick::default());
    app.update();
    assert!(roots(&app).is_empty());
    assert!(app.world().get_entity(left).is_err());
}

#[test]
fn a_stand_in_is_taken_by_the_nearest_tick_within_the_slack_of_its_unit_and_type() {
    // Stand-ins of tick 10 and 12 of one unit and type, and one of tick 11 of another type.
    let mut ids = IdAllocator::default();
    let [source, other] = [ids.allocate(), ids.allocate()];
    let mut world = World::new();
    let stand_in = |world: &mut World, unit_type, fired| StandIn {
        root: world.spawn_empty().id(),
        source,
        unit_type,
        from: Vec3::ZERO,
        step: Vec3::ZERO,
        fired: Tick::new(fired),
    };
    let ten = stand_in(&mut world, SHELL, 10);
    let twelve = stand_in(&mut world, SHELL, 12);
    let bolt = stand_in(&mut world, SHOOTER, 11);
    let mut stand_ins = ShotStandIns(vec![ten, twelve, bolt]);
    let take = |stand_ins: &mut ShotStandIns, source, unit_type, tick| {
        stand_ins.take(source, unit_type, Tick::new(tick))
    };
    // At 11, both are 1 apart: the older goes first. Another unit's takes none, nor a tick 6
    // past the one left, 12.
    assert_eq!(take(&mut stand_ins, source, SHELL, 11), Some(ten.root));
    assert_eq!(take(&mut stand_ins, other, SHELL, 12), None);
    assert_eq!(take(&mut stand_ins, source, SHELL, 18), None);
    // 5 past it takes it; the other type's stays.
    assert_eq!(take(&mut stand_ins, source, SHELL, 17), Some(twelve.root));
    assert_eq!(stand_ins.0, [bolt]);
}
