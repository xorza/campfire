use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;

use bevy::math::Quat;
use campfire_client_files::{EffectList, EffectSets, EffectsFile, ListedSystem, SlaveSystem};

use super::*;
use crate::view::effects::particle_run::samples::{book, name, system};

/// Starts the system `text` of `book` at `at`.
fn start(run: &mut ParticleRun, book: &EffectBook, text: &str, at: Vec3) -> SystemKey {
    run.start(
        book,
        book.id(&name(text)).unwrap(),
        Affine3A::from_translation(at),
    )
}

fn level(_: f32, _: f32) -> f32 {
    0.0
}

#[test]
fn a_burst_comes_after_the_system_s_delay_then_every_burst_delay_and_a_step() {
    // A delay of 2 steps, then a burst of 2 particles, then 3 steps of the burst's delay before
    // the next: bursts at the 3rd and the 7th steps.
    let smoke = book(
        100,
        [(
            "smoke",
            ParticleSystem {
                initial_delay: Spread::of(2.0),
                burst_delay: Spread::of(3.0),
                burst_count: Spread::of(2.0),
                ..system()
            },
        )],
    );
    let mut run = ParticleRun::new(1);
    start(&mut run, &smoke, "smoke", Vec3::ZERO);
    let alive: Vec<u32> = (0..8)
        .map(|_| {
            run.step(&smoke, level);
            run.alive
        })
        .collect();
    assert_eq!(alive, [0, 0, 2, 2, 2, 2, 4, 4]);

    // A system of a life of 2 steps bursts twice, then goes once its particles went: each lives
    // 3 steps, the step it is made in its first, so the first goes at the 3rd, the second at the
    // 4th.
    let flash = book(
        100,
        [(
            "flash",
            ParticleSystem {
                system_lifetime: 2,
                lifetime: Spread::of(3.0),
                ..system()
            },
        )],
    );
    let mut run = ParticleRun::new(1);
    start(&mut run, &flash, "flash", Vec3::ZERO);
    let alive: Vec<u32> = (0..6)
        .map(|_| {
            run.step(&flash, level);
            run.alive
        })
        .collect();
    assert_eq!(alive, [1, 2, 1, 0, 0, 0]);
    assert!(run.order.is_empty());

    // A stopped system emits none until it resumes; scaled, a burst of 4 by 0.5 makes 2, each of
    // the velocity (1, 2, 4) by (0.5, 0.25, 0.5), (0.5, 0.5, 2); by 0.3, 1.2, truncated to 1.
    let dust = book(
        100,
        [(
            "dust",
            ParticleSystem {
                burst_count: Spread::of(4.0),
                velocity: EmissionVelocity::Ortho {
                    x: Spread::of(1.0),
                    y: Spread::of(2.0),
                    z: Spread::of(4.0),
                },
                ..system()
            },
        )],
    );
    let mut run = ParticleRun::new(1);
    let key = start(&mut run, &dust, "dust", Vec3::ZERO);
    run.stop(key);
    run.step(&dust, level);
    assert_eq!(run.alive, 0);
    run.resume(key);
    run.scale(key, Vec3::new(0.5, 0.25, 0.5), 0.5);
    run.step(&dust, level);
    assert_eq!(run.alive, 2);
    let scaled = Vec3::new(0.5, 0.5, 2.0);
    assert!(run.particles().all(|particle| particle.velocity == scaled));
    run.scale(key, Vec3::ONE, 0.3);
    run.step(&dust, level);
    assert_eq!(run.alive, 3);
}

#[test]
fn a_particle_starts_on_a_hollow_cylinder_and_its_velocity_is_damped_and_pulled() {
    // A hollow cylinder of radius 2 and length 4 about (10, 0, 0), a velocity of 1 along `x`,
    // damped by half and pulled by −0.1 along `y` each step: (1, −0.1) · 0.5 = (0.5, −0.05) at
    // the first step, then (0.5, −0.15) · 0.5 = (0.25, −0.075).
    let book = book(
        100,
        [(
            "dust",
            ParticleSystem {
                volume: EmissionVolume::Cylinder {
                    radius: 2.0,
                    length: 4.0,
                },
                hollow: true,
                velocity: EmissionVelocity::Ortho {
                    x: Spread::of(1.0),
                    y: Spread::ZERO,
                    z: Spread::ZERO,
                },
                velocity_damping: Spread::of(0.5),
                gravity: -0.1,
                system_lifetime: 1,
                ..system()
            },
        )],
    );
    let mut run = ParticleRun::new(7);
    start(&mut run, &book, "dust", Vec3::new(10.0, 0.0, 0.0));
    run.step(&book, level);
    let first = run.particles().next().unwrap().position;
    let start = first - Vec3::new(0.5, -0.05, 0.0);
    let out = start - Vec3::new(10.0, start.y, 0.0);
    assert!((out.length() - 2.0).abs() < 1e-5, "{start}");
    assert!((-2.0..=2.0).contains(&start.y));
    run.step(&book, level);
    let second = run.particles().next().unwrap().position;
    assert!(
        (second - first).abs_diff_eq(Vec3::new(0.25, -0.075, 0.0), 1e-6),
        "{second}"
    );
}

#[test]
fn a_particle_s_size_grows_by_its_damped_rate_and_its_alpha_and_color_run_between_keys() {
    // Size 2, growing by 1 a step, its rate halved each step past its first: 3, 3.5, 3.75.
    // Alpha from 0 at step 0 to 1 at 4, then to 0.5 at 8: 0.25 a step, landed on 1 at its key's
    // step, then −0.125 a step. Color from red at 0 to blue at 2: half a step, each share held
    // between 0 and 1.
    let book = book(
        100,
        [(
            "glow",
            ParticleSystem {
                size: Spread::of(2.0),
                size_rate: Spread::of(1.0),
                size_rate_damping: Spread::of(0.5),
                alpha: vec![
                    AlphaKey {
                        alpha: Spread::ZERO,
                        at: 0,
                    },
                    AlphaKey {
                        alpha: Spread::of(1.0),
                        at: 4,
                    },
                    AlphaKey {
                        alpha: Spread::of(0.5),
                        at: 8,
                    },
                ],
                color: vec![
                    ColorKey {
                        color: [1.0, 0.0, 0.0],
                        at: 0,
                    },
                    ColorKey {
                        color: [0.0, 0.0, 1.0],
                        at: 2,
                    },
                ],
                system_lifetime: 1,
                ..system()
            },
        )],
    );
    let mut run = ParticleRun::new(3);
    start(&mut run, &book, "glow", Vec3::ZERO);
    let mut seen = Vec::new();
    for _ in 0..7 {
        run.step(&book, level);
        let particle = run.particles().next().unwrap();
        seen.push((particle.size, particle.alpha, particle.color.to_array()));
    }
    assert_eq!(
        seen,
        [
            (3.0, 0.25, [0.5, 0.0, 0.5]),
            (3.5, 0.5, [0.0, 0.0, 1.0]),
            (3.75, 0.75, [0.0, 0.0, 1.0]),
            (3.875, 1.0, [0.0, 0.0, 1.0]),
            (3.9375, 1.0, [0.0, 0.0, 1.0]),
            (3.96875, 0.875, [0.0, 0.0, 1.0]),
            (3.984375, 0.75, [0.0, 0.0, 1.0]),
        ]
    );
}

#[test]
fn past_the_cap_a_particle_removes_the_oldest_of_a_lower_rank_or_is_not_made() {
    // A cap of 3: three particles of rank 0, then two of rank 1. The fourth passes the cap, as
    // the game removes only past it; the fifth removes the oldest of rank 0.
    let low = ParticleSystem {
        burst_count: Spread::of(3.0),
        system_lifetime: 1,
        ..system()
    };
    let high = ParticleSystem {
        rank: Some(1),
        burst_count: Spread::of(2.0),
        system_lifetime: 1,
        ..system()
    };
    let always = ParticleSystem {
        rank: None,
        system_lifetime: 1,
        ..system()
    };
    let book = book(3, [("low", low), ("high", high), ("always", always)]);
    let mut run = ParticleRun::new(5);
    start(&mut run, &book, "low", Vec3::ZERO);
    run.step(&book, level);
    start(&mut run, &book, "high", Vec3::ZERO);
    run.step(&book, level);
    let mut made: Vec<(Option<u8>, u64)> = run
        .particles()
        .map(|particle| (particle.rank, particle.made))
        .collect();
    made.sort_unstable();
    assert_eq!(
        made,
        [(Some(0), 1), (Some(0), 2), (Some(1), 3), (Some(1), 4)]
    );
    // A particle of rank 0 finds none lower to remove, and is not made; one of no rank passes
    // the cap.
    start(&mut run, &book, "low", Vec3::ZERO);
    start(&mut run, &book, "always", Vec3::ZERO);
    run.step(&book, level);
    assert_eq!(run.alive, 5);
    assert_eq!(
        run.particles()
            .filter(|particle| particle.rank.is_none())
            .count(),
        1
    );
}

#[test]
fn a_system_s_slave_takes_a_particle_of_each_beside_it_and_a_particle_carries_its_system() {
    // The spark's slave, the ember, takes a particle 1 above each spark's, of its own life of 5
    // steps; each spark carries a trail, which rides it.
    let book = book(
        100,
        [
            (
                "spark",
                ParticleSystem {
                    slave: Some(SlaveSystem {
                        system: name("ember"),
                        offset: Vec3::Y,
                    }),
                    attached: Some(name("trail")),
                    velocity: EmissionVelocity::Ortho {
                        x: Spread::of(1.0),
                        y: Spread::ZERO,
                        z: Spread::ZERO,
                    },
                    system_lifetime: 1,
                    ..system()
                },
            ),
            (
                "ember",
                ParticleSystem {
                    lifetime: Spread::of(5.0),
                    ..system()
                },
            ),
            (
                "trail",
                ParticleSystem {
                    rank: Some(2),
                    ..system()
                },
            ),
        ],
    );
    let mut run = ParticleRun::new(9);
    start(&mut run, &book, "spark", Vec3::ZERO);
    run.step(&book, level);
    let ember = book.id(&name("ember")).unwrap();
    let trail = book.id(&name("trail")).unwrap();
    let of = |run: &ParticleRun, id| {
        run.particles()
            .filter(|particle| run.system(particle.system).id == id)
            .map(|particle| particle.position)
            .collect::<Vec<_>>()
    };
    // The spark moved 1 along `x`; the ember, of the spark's velocity, too, from 1 above.
    assert_eq!(of(&run, ember), [Vec3::new(1.0, 1.0, 0.0)]);
    assert_eq!(
        run.particles()
            .find(|particle| particle.life_left == 4)
            .map(|particle| particle.position),
        Some(Vec3::new(1.0, 1.0, 0.0))
    );
    // The trail started at the spark's start and emitted there, then rides the spark. A
    // burst's first particle starts where its system stood a step before, as the game spreads a
    // burst along a moving system's move: the 2nd at the spark's start, the 3rd at its first
    // step's place.
    assert_eq!(of(&run, trail), [Vec3::ZERO]);
    run.step(&book, level);
    run.step(&book, level);
    let mut trails = of(&run, trail);
    trails.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(trails, [Vec3::ZERO, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)]);
}

#[test]
fn a_list_starts_its_systems_turned_and_offset_as_its_source_and_delayed_as_it_says() {
    // A list of the smoke twice, 1 ahead of its source along `x` and 2 above it, turned with its
    // source, after 2.5 steps, 3 rounded up, and on its object; and of the flash on the ground.
    let smoke = ListedSystem {
        system: name("smoke"),
        count: 2,
        offset: Vec3::X,
        radius: Spread::ZERO,
        height: Spread::of(2.0),
        delay: Some(Spread::of(2.5)),
        rotation: Quat::IDENTITY,
        orient_to_object: true,
        attach_to_object: true,
        at_ground: false,
        callers_radius: false,
        ricochet: false,
    };
    let flash = ListedSystem {
        system: name("flash"),
        count: 1,
        offset: Vec3::ZERO,
        delay: None,
        orient_to_object: false,
        attach_to_object: false,
        at_ground: true,
        ..smoke
    };
    let systems = BTreeMap::from([(name("smoke"), system()), (name("flash"), system())]);
    let lists = BTreeMap::from([(
        name("hit"),
        EffectList {
            systems: vec![smoke, flash],
            sounds: Vec::new(),
        },
    )]);
    let sets = EffectSets {
        systems,
        sounds: BTreeMap::new(),
        lists,
    };
    let book = EffectBook::new(&EffectsFile::new(1.0 / 30.0, 100, sets).unwrap());
    let mut run = ParticleRun::new(2);
    // The source at (10, 0, 0), turned a right angle about `y`: its `x` the world's `−z`.
    let source = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2),
        Vec3::new(10.0, 0.0, 0.0),
    );
    run.play(
        &book,
        book.list_id(&name("hit")).unwrap(),
        source,
        |_, _| 5.0,
    );
    let started: Vec<(SystemId, Vec3, u32)> = run
        .order
        .iter()
        .map(|&slot| {
            let system = run.system(slot);
            (
                system.id,
                Vec3::from(system.placement.translation),
                system.delay_left,
            )
        })
        .collect();
    let [smoke, flash] = [name("smoke"), name("flash")].map(|of| book.id(&of).unwrap());
    let ahead = Vec3::new(10.0, 2.0, -1.0);
    assert_eq!(started.len(), 3);
    for (at, expected) in started.iter().zip([
        (smoke, ahead, 3),
        (smoke, ahead, 3),
        (flash, Vec3::new(10.0, 5.0, 0.0), 0),
    ]) {
        assert_eq!(at.0, expected.0);
        assert!(at.1.abs_diff_eq(expected.1, 1e-5), "{:?}", at.1);
        assert_eq!(at.2, expected.2);
    }
    // The smoke turns with its source, the flash does not; the smokes follow their object.
    let turned = run
        .system(run.order[0])
        .placement
        .transform_vector3(Vec3::X);
    assert!(turned.abs_diff_eq(Vec3::NEG_Z, 1e-6), "{turned}");
    assert_eq!(
        run.system(run.order[2])
            .placement
            .transform_vector3(Vec3::X),
        Vec3::X
    );
    assert_eq!(run.attached().len(), 2);
    // Of no list, one plays nothing; of one, it plays that list: its 3 systems again.
    let hit = book.list_id(&name("hit")).unwrap();
    run.play_one(&book, &[], source, |_, _| 5.0);
    assert_eq!(run.order.len(), 3);
    run.play_one(&book, &[hit], source, |_, _| 5.0);
    assert_eq!(run.order.len(), 6);
}

#[test]
fn a_moved_system_s_burst_spreads_along_its_move_since_its_last_step() {
    // Two particles a burst; the system moves from the origin to (2, 0, 0) between steps: the
    // second burst's first starts where it stood, the second halfway, (1, 0, 0).
    let book = book(
        100,
        [(
            "puff",
            ParticleSystem {
                burst_count: Spread::of(2.0),
                ..system()
            },
        )],
    );
    let mut run = ParticleRun::new(4);
    let key = start(&mut run, &book, "puff", Vec3::ZERO);
    run.step(&book, level);
    run.place(key, Affine3A::from_translation(Vec3::new(2.0, 0.0, 0.0)));
    run.step(&book, level);
    let mut xs: Vec<f32> = run
        .particles()
        .map(|particle| particle.position.x)
        .collect();
    xs.sort_by(f32::total_cmp);
    assert_eq!(xs, [0.0, 0.0, 0.0, 1.0]);
}
