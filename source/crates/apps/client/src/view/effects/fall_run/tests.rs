use std::f32::consts::{FRAC_PI_2, PI};

use super::*;

/// A step of 1/32 s, exact in binary.
const STEP: f32 = 1.0 / 32.0;

/// A spiral of no spin: a push of 32 m/s² from `x` that turns a quarter turn a second and halves
/// each step, a brake of 64 m/s² forward and 32 m/s² backward.
const SPIRAL: SpiralPlay = SpiralPlay {
    push: 32.0,
    turn: FRAC_PI_2,
    fade: 0.5,
    braking: 64.0,
    acceleration: 32.0,
    spin: SpinPlay {
        least: 0.0,
        most: 0.0,
        change: 0.0,
        every: 0.0,
    },
};

fn play(spiral: Option<SpiralPlay>, roll: f32, ground: Option<GroundPlay>) -> FallPlay {
    FallPlay {
        gravity: 0.5,
        roll,
        spiral,
        start: MomentPlay::default(),
        hit: MomentPlay::default(),
        blow_up_after: 0.25,
        blow_up: MomentPlay::default(),
        ground,
    }
}

#[test]
fn an_aircraft_falls_by_its_share_hits_the_ground_and_blows_up_after() {
    let jet = UnitType::new(1);
    let heli = UnitType::new(2);
    let plays = BTreeMap::from([
        (
            jet,
            play(
                None,
                FRAC_PI_2,
                Some(GroundPlay {
                    height: 0.64,
                    moment: MomentPlay::default(),
                }),
            ),
        ),
        (heli, play(Some(SPIRAL), 0.0, None)),
    ]);
    let mut run = FallRun::default();
    let [jet_root, heli_root, parked] = [1, 2, 3].map(Entity::from_raw_u32).map(Option::unwrap);
    let from = |root, unit_type, y| FallFrom {
        root,
        unit_type,
        at: Affine3A::from_translation(Vec3::new(0.0, y, 0.0)),
        velocity: Vec3::new(4.0, 0.0, 0.0),
    };
    // A jet at most its ground death's 0.64 m above the ground plays it alone; above, it falls.
    assert_eq!(
        run.start(from(parked, jet, 0.64), &plays[&jet], 0.0),
        FallStart::OnGround(&plays[&jet].ground.as_ref().unwrap().moment)
    );
    assert_eq!(
        run.start(from(jet_root, jet, 1.0), &plays[&jet], 0.0),
        FallStart::Falls(&plays[&jet].start)
    );
    // Each keeps its velocity.
    assert_eq!(
        run.start(from(heli_root, heli, 100.0), &plays[&heli], 0.0),
        FallStart::Falls(&plays[&heli].start)
    );
    assert_eq!(run.falls()[0].velocity, Vec3::new(4.0, 0.0, 0.0));
    assert_eq!(run.falls()[1].velocity, Vec3::new(4.0, 0.0, 0.0));

    // Gravity of −64 at a share of 0.5: −1 a step's speed down, so after step k the jet stands at
    // 1 − k(k+1)/64, first at or below 0 at k = 8, 1 − 72/64 = −0.125, at 4 × 8/32 = 1 along x;
    // it rolled a quarter turn a second, π/2 × 8/32 = π/8.
    let ground = |_: f32, _: f32| 0.0;
    let mut events = Vec::new();
    for _ in 0..7 {
        run.step(STEP, -64.0, &plays, &ground, &mut events);
    }
    assert_eq!(events, []);
    assert_eq!(run.falls()[0].at.y, 1.0 - 56.0 / 64.0);
    run.step(STEP, -64.0, &plays, &ground, &mut events);
    let hit = Transform::from_xyz(1.0, 0.0, 0.0)
        .with_rotation(Quat::from_rotation_x(FRAC_PI_2 * 8.0 / 32.0))
        .compute_affine();
    let [
        FallEvent::Hit {
            root,
            unit_type,
            at,
        },
    ] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!((*root, *unit_type), (jet_root, jet));
    assert!(at.abs_diff_eq(hit, 1e-5), "{at:?}");
    // A quarter second after its hit, 8 steps, the jet blows up where it lies, and goes.
    events.clear();
    for _ in 0..7 {
        run.step(STEP, -64.0, &plays, &ground, &mut events);
    }
    assert_eq!(events, []);
    run.step(STEP, -64.0, &plays, &ground, &mut events);
    assert!(matches!(events.as_slice(), [FallEvent::BlowUp { root, .. }] if *root == jet_root));
    assert_eq!(run.falls().len(), 1);
}

#[test]
fn a_helicopter_spirals_by_its_push_its_brake_and_its_spin() {
    let spiral = play(Some(SPIRAL), 0.0, None);
    let start = |velocity: Vec3| {
        let mut run = FallRun::default();
        let from = FallFrom {
            root: Entity::from_raw_u32(1).unwrap(),
            unit_type: UnitType::new(1),
            at: Affine3A::from_translation(Vec3::new(0.0, 100.0, 0.0)),
            velocity,
        };
        run.start(from, &spiral, 0.0);
        run.falls[0].clone()
    };
    let flat = |fall: &Falling| Vec3::new(fall.velocity.x, 0.0, fall.velocity.z);
    // Facing x at 4 m/s, a step of 1/32 s: its brake takes 64/32 = 2 of its 4 along x, and its
    // push of 32 gives 1 along x, its way at 0: 3. Then its way turned π/64 and its push halved:
    // 3 − 2 + ½ cos(π/64) along x and −½ sin(π/64) along z, as its way turns from x toward −z.
    let mut heli = start(Vec3::new(4.0, 0.0, 0.0));
    heli.step(STEP, -64.0, &spiral, 0.0);
    assert_eq!(flat(&heli), Vec3::new(3.0, 0.0, 0.0));
    heli.step(STEP, -64.0, &spiral, 0.0);
    let half = Vec3::new(1.0 + 0.5 * (PI / 64.0).cos(), 0.0, -0.5 * (PI / 64.0).sin());
    assert!((flat(&heli) - half).length() < 1e-6, "{:?}", flat(&heli));
    // From the third step its brake takes all its speed along x, which is below 2, and its push
    // of 32 ÷ 2^(k − 1) at (k − 1)π/64 gives the rest: after the 8th, 2^−7 cos(7π/64) along x and
    // −Σ 2^−j sin(jπ/64), j from 1 to 7, along z; it fell 8 steps of 1 m/s each, −8.
    for _ in 3..=8 {
        heli.step(STEP, -64.0, &spiral, 0.0);
    }
    let z: f32 = (1..=7)
        .map(|j| -(0.5_f32.powi(j)) * (j as f32 * PI / 64.0).sin())
        .sum();
    let eighth = Vec3::new(0.5_f32.powi(7) * (7.0 * PI / 64.0).cos(), 0.0, z);
    assert!((flat(&heli) - eighth).length() < 1e-6, "{:?}", flat(&heli));
    assert_eq!(heli.velocity.y, -8.0);
    // Backing at 4 m/s, its brake gives back 32/32 = 1, its acceleration's, not its braking's 2,
    // and its push 1: −2.
    let mut backing = start(Vec3::new(-4.0, 0.0, 0.0));
    backing.step(STEP, -64.0, &spiral, 0.0);
    assert_eq!(flat(&backing), Vec3::new(-2.0, 0.0, 0.0));
}

#[test]
fn a_spirals_spin_swings_between_its_least_and_its_most() {
    // A spin of 1 to 2 rad/s by 0.5, each 3 steps of 1/32 s past its last change: it changes in
    // its first step, then each 4th, 1.5, 2, then past 2 back at 2 and falling, 1.5, 1, then past
    // 1 back at 1 and rising.
    let spin = SpinPlay {
        least: 1.0,
        most: 2.0,
        change: 0.5,
        every: 3.0 / 32.0,
    };
    let mut orbit = Orbit {
        way: 0.0,
        push: 0.0,
        spin: 1.0,
        rising: true,
        since: u32::MAX,
    };
    let mut rates = Vec::new();
    for _ in 0..21 {
        orbit.spin_on(spin, STEP);
        rates.push(orbit.spin);
    }
    let changes: Vec<f32> = [0, 4, 8, 12, 16, 20].map(|at| rates[at]).into();
    assert_eq!(changes, [1.5, 2.0, 2.0, 1.5, 1.0, 1.0]);
    assert_eq!(rates[3], 1.5);
    assert!(orbit.rising);
    // A spin of no wait between its changes changes never.
    let mut fixed = Orbit {
        spin: 1.0,
        since: u32::MAX,
        ..orbit
    };
    fixed.spin_on(SpinPlay { every: 0.0, ..spin }, STEP);
    assert_eq!(fixed.spin, 1.0);
    // A fall spins its facing at its rate: at a steady 1 rad/s, 8 steps turn it 8/32 rad.
    let steady = play(
        Some(SpiralPlay {
            spin: SpinPlay {
                least: 1.0,
                most: 1.0,
                change: 0.0,
                every: 0.0,
            },
            ..SPIRAL
        }),
        0.0,
        None,
    );
    let mut run = FallRun::default();
    let from = FallFrom {
        root: Entity::from_raw_u32(1).unwrap(),
        unit_type: UnitType::new(1),
        at: Affine3A::from_translation(Vec3::new(0.0, 100.0, 0.0)),
        velocity: Vec3::ZERO,
    };
    run.start(from, &steady, 0.0);
    let mut fall = run.falls[0].clone();
    for _ in 0..8 {
        fall.step(STEP, -64.0, &steady, 0.0);
    }
    assert!(
        fall.turn
            .abs_diff_eq(Quat::from_rotation_y(8.0 / 32.0), 1e-6),
        "{:?}",
        fall.turn
    );
}
