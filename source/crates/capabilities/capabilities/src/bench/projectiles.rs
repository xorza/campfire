use std::time::{Duration, Instant};

use campfire_combat::Combat;
use campfire_combat::internals::Armed;
use campfire_declarations::Role;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_projectiles::{ProjectileData, Projectiles, SteerData};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimWorld, StableId};
use campfire_units::internals::Spawn;
use campfire_units::{BodyForm, BodyShape, Layer, UnitTypeCore, Units};
use campfire_values::{Team, TypeScope};
use criterion::{Criterion, Throughput};

use crate::release::r#match::Match;

/// Missiles in flight in `steered`.
const MISSILES: i64 = 1000;
/// The ticks a scene of `steered` flies before it is made again: its missiles, half a meter a tick
/// toward targets 400 m off and more, fly 800 ticks at the least, so none ends within them.
const FLIGHT_TICKS: u32 = 600;

/// A tick of 1,000 steered missiles in flight, `steered`, the match's schedule as it runs it: each
/// of 15 m/s, half a meter a tick, turning 90° a second, launched 1 m up from a grid of 40 by 25 m
/// at one of 10 targets 400 m along x, which then move 200 m along z, so each turns as it flies;
/// a tick steps every missile and meets the units near its point. A scene flies 600 ticks, before
/// any missile reaches its target, and is made again outside the measure.
pub(super) fn projectiles(c: &mut Criterion) {
    let mut scene: Option<(Match, u32)> = None;
    let mut group = Kind::Integration.group(c, "projectiles", Scale::Frame);
    group.throughput(Throughput::Elements(MISSILES.unsigned_abs()));
    group.bench_function("steered", |bench| {
        bench.iter_custom(|iters| {
            let mut spent = Duration::ZERO;
            for _ in 0..iters {
                if scene
                    .as_ref()
                    .is_none_or(|(_, flown)| *flown >= FLIGHT_TICKS)
                {
                    scene = Some((missiles(), 0));
                }
                let (world, flown) = scene.as_mut().expect("a scene made");
                let start = Instant::now();
                world.step();
                spent += start.elapsed();
                *flown += 1;
            }
            spent
        });
    });
    group.finish();
}

/// The missiles of `steered`, launched, their targets moved.
fn missiles() -> Match {
    let mut world = Match::builder(&[Capability::Combat, Capability::Projectiles])
        .role(Role::Rules)
        .build();
    Combat::load_weapon_stats(&mut world);
    let missile = Units::load_type(
        &mut world,
        TypeScope::Mode,
        "missile",
        &UnitTypeCore::default(),
    );
    let steer = SteerData::builder(Num::int(90), 60_000).build().unwrap();
    let data = ProjectileData {
        steer: Some(steer),
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut world, missile, &data);
    let round = Armed::unarmed(Num::int(100)).load(&mut world);
    Units::give_type(
        &mut world,
        round,
        BodyForm::new(BodyShape::circle(Num::ONE).unwrap(), Layer::FIRST),
    );
    let source = Spawn::of(round, (0, 0, 0)).place(&mut world);
    let targets: Vec<StableId> = (0..10)
        .map(|nth| {
            Spawn::of(round, (400, 0, 3 * nth))
                .team(Team::new(1))
                .place(&mut world)
        })
        .collect();
    for nth in 0..MISSILES {
        let from = Vec3::new(Num::int(nth % 40), Num::ONE, Num::int(nth / 40));
        let target = targets[usize::try_from(nth % 10).unwrap()];
        Projectiles::fire_steered(&mut world, source, missile, from, target);
    }
    world.step();
    for (nth, &target) in (0..).zip(&targets) {
        let entity = world.unit_entity(target);
        *world.get_mut::<Position>(entity).unwrap() =
            Position::from(Vec3::new(Num::int(400), Num::ZERO, Num::int(200 + 3 * nth)));
    }
    world
}
