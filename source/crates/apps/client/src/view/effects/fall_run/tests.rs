use std::f32::consts::FRAC_PI_2;

use super::*;

/// A step of 1/32 s, exact in binary.
const STEP: f32 = 1.0 / 32.0;

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
        (
            heli,
            play(
                Some(SpiralPlay {
                    turn: FRAC_PI_2,
                    speed: 8.0,
                    spin: 1.0,
                }),
                0.0,
                None,
            ),
        ),
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
    // The jet keeps its velocity; the helicopter starts along its facing, x, at its spiral's
    // speed.
    assert_eq!(
        run.start(from(heli_root, heli, 100.0), &plays[&heli], 0.0),
        FallStart::Falls(&plays[&heli].start)
    );
    assert_eq!(run.falls()[0].velocity, Vec3::new(4.0, 0.0, 0.0));
    assert_eq!(run.falls()[1].velocity, Vec3::new(8.0, 0.0, 0.0));

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
    // The helicopter turned its path a quarter turn a second: after 8 steps, π/8 from x.
    let heli_fall = &run.falls()[1];
    let path = Quat::from_rotation_y(FRAC_PI_2 * 8.0 / 32.0) * Vec3::new(8.0, 0.0, 0.0);
    let flat = Vec3::new(heli_fall.velocity.x, 0.0, heli_fall.velocity.z);
    assert!((flat - path).length() < 1e-4, "{flat:?}");
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
