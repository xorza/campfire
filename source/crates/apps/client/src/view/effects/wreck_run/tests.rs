use std::f32::consts::FRAC_PI_2;

use campfire_math::Num;

use super::*;

/// Zero Hour's ground: gravity of −64 a second squared, a stiffness of 0.8, a friction of 0.15
/// a step and a rest below 15 a second.
const GROUND: WreckGround = WreckGround {
    gravity: -64.0,
    stiffness: 0.8,
    friction: 0.15,
    rest: 15.0,
};

/// A step of 1/32 s, exact in binary, near the game's frame.
const STEP: f32 = 1.0 / 32.0;

fn piece(velocity: Vec3, life: Option<f32>) -> WreckPiece {
    WreckPiece {
        entity: Entity::PLACEHOLDER,
        at: Vec3::new(0.0, 1.0, 0.0),
        velocity,
        turn: Quat::IDENTITY,
        spin: Vec3::ZERO,
        age: 0.0,
        stage: PieceStage::Moving,
        end: WreckEnd {
            life,
            delay: 0.25,
            rate: 2.0,
            gone: 0.5,
        },
    }
}

#[test]
fn a_piece_falls_lands_slows_rests_sinks_and_goes() {
    // Steps of 1/32 s, exact in binary. Debris at 1 m flying at 32 across and 16 up: after the
    // first step its speed up is 16 − 64/32 = 14 and it stands at x = 1, y = 1 + 14/32 = 1.4375.
    let mut debris = piece(Vec3::new(32.0, 16.0, 0.0), None);
    assert!(debris.step(STEP, GROUND, 0.0));
    assert_eq!(debris.velocity, Vec3::new(32.0, 14.0, 0.0));
    assert_eq!(debris.at, Vec3::new(1.0, 1.4375, 0.0));
    // It rises while its speed up lasts, then falls back: it lands on the step whose move takes
    // it to the ground.
    let mut steps = 1;
    while debris.at.y > 0.0 {
        assert!(debris.step(STEP, GROUND, 0.0));
        steps += 1;
    }
    // From 1 m, up at 16 and down by 2 a step: after step k it stands at
    // 1 + (16k − k(k+1))/32, first at or below 0 at k = 17: 1 + (272 − 306)/32 = −0.0625; its
    // speed down then −18, of which the ground leaves a fifth, −3.6, and its speed across 32,
    // of which the friction leaves 0.85, 27.2.
    assert_eq!(steps, 17);
    assert_eq!(debris.at.y, 0.0);
    assert!(
        (debris.velocity - Vec3::new(27.2, -3.6, 0.0)).length() < 1e-4,
        "{:?}",
        debris.velocity
    );
    assert_eq!(debris.stage, PieceStage::Moving);
    // On the ground, each step leaves 0.85 of its speed across, which falls below 15 after
    // 27.2 × 0.85ⁿ < 15, n = 4 more steps: 27.2 × 0.85⁴ = 14.199; there it rests and its end
    // begins.
    for _ in 0..3 {
        debris.step(STEP, GROUND, 0.0);
        assert_eq!(debris.stage, PieceStage::Moving);
    }
    debris.step(STEP, GROUND, 0.0);
    let rested = debris.age;
    assert_eq!(debris.stage, PieceStage::Ended { since: rested });
    assert_eq!(debris.velocity, Vec3::ZERO);
    // It sinks from the step a quarter second after it rests, the 8th, at 2 a second, 1/16 a
    // step, and goes half a second after its rest, at the 16th.
    for _ in 0..7 {
        debris.step(STEP, GROUND, 0.0);
    }
    assert_eq!(debris.at.y, 0.0);
    debris.step(STEP, GROUND, 0.0);
    assert_eq!(debris.at.y, -0.0625);
    let mut more = 0;
    while debris.step(STEP, GROUND, 0.0) {
        more += 1;
    }
    assert_eq!(more, 7, "gone at the 16th step after its rest");

    // A hulk still on the ground rests until its life of 1/8 s ends, 4 steps, then sinks after its
    // delay as debris does.
    let mut hulk = piece(Vec3::ZERO, Some(0.125));
    hulk.at.y = 0.0;
    for _ in 0..3 {
        assert!(hulk.step(STEP, GROUND, 0.0));
        assert_ne!(hulk.stage, PieceStage::Ended { since: hulk.age });
    }
    hulk.step(STEP, GROUND, 0.0);
    assert_eq!(hulk.stage, PieceStage::Ended { since: 0.125 });
}

#[test]
fn a_throw_launches_each_piece_as_its_play_says() {
    let scene = AssetPath::from("package://0/client/models/hulk.glb#Scene0");
    let play = |launch| WreckPlay {
        scenes: vec![scene.clone()],
        count: 2,
        offset: Vec3::new(1.0, 0.0, 0.0),
        launch,
        spin: 0.0,
        end: WreckEnd {
            life: None,
            delay: 0.0,
            rate: 0.0,
            gone: 1.0,
        },
    };
    let num = |value: i64| Num::int(value);
    // A drawing at (10, 5, 0) turned a quarter about up: the offset along its facing, x, turns to
    // −z.
    let at = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2),
        Vec3::new(10.0, 5.0, 0.0),
    );
    let ground = |_: f32, _: f32| 2.0;
    let mut run = WreckRun::new(7);
    let mut spawned = Vec::new();
    let launches = [
        WreckLaunch::Still,
        WreckLaunch::Grounded,
        WreckLaunch::Out { across: num(3) },
        WreckLaunch::Fling {
            across: num(4),
            up: [num(5), num(6)],
        },
        WreckLaunch::Force {
            speed: [num(10), num(10)],
            pitch: [Num::ZERO, Num::ZERO],
        },
    ];
    for launch in launches {
        run.throw(&[play(launch)], at, &ground, |scene, place| {
            spawned.push((scene.clone(), place));
            Entity::PLACEHOLDER
        });
    }
    let pieces = run.pieces();
    assert_eq!((pieces.len(), spawned.len()), (10, 10));
    let near = |a: Vec3, b: Vec3| (a - b).length() < 1e-5;
    // Still, where the offset puts it, turned as its drawing, of no velocity.
    assert!(
        near(pieces[0].at, Vec3::new(10.0, 5.0, -1.0)),
        "{:?}",
        pieces[0].at
    );
    assert_eq!(pieces[0].velocity, Vec3::ZERO);
    // On the ground beneath, still.
    assert_eq!((pieces[2].at.y, pieces[2].velocity), (2.0, Vec3::ZERO));
    // Out: on the ground, each ground axis within ±3, none up.
    let out = pieces[4].velocity;
    assert!(
        out.x.abs() <= 3.0 && out.z.abs() <= 3.0 && out.y == 0.0,
        "{out:?}"
    );
    // Flung: across within ±4, up within 5 to 6.
    let fling = pieces[6].velocity;
    assert!(fling.x.abs() <= 4.0 && fling.z.abs() <= 4.0, "{fling:?}");
    assert!((5.0..=6.0).contains(&fling.y), "{fling:?}");
    // Forced at 10 with no pitch: 10 along the ground, none up.
    let force = pieces[8].velocity;
    assert!(
        (Vec3::new(force.x, 0.0, force.z).length() - 10.0).abs() < 1e-4 && force.y.abs() < 1e-5
    );
    assert!(spawned.iter().all(|(drawn, _)| *drawn == scene));
}
