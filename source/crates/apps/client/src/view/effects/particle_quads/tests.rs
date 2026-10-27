use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;

use bevy::math::Affine3A;
use campfire_client_files::{
    AlphaKey, ColorKey, EffectSets, EffectsFile, EmissionVelocity, EmissionVolume, ParticleSystem,
    Spread,
};
use campfire_common::DeclaredName;
use campfire_values::PackagePath;

use super::*;

/// A system of one particle at rest, of `size`, turned `angle`, red at an alpha of half, of the
/// texture `texture`, drawn in `blend`, lying on the ground when `ground_aligned`.
fn system(
    texture: &str,
    blend: Blend,
    size: f32,
    angle: f32,
    ground_aligned: bool,
) -> ParticleSystem {
    ParticleSystem {
        rank: None,
        blend,
        texture: PackagePath::parse(texture).unwrap(),
        ground_aligned,
        up_towards_emitter: false,
        above_ground_only: false,
        angle: Spread::of(angle),
        angular_rate: Spread::ZERO,
        angular_damping: Spread::of(1.0),
        velocity_damping: Spread::of(1.0),
        gravity: 0.0,
        drift: Vec3::ZERO,
        lifetime: Spread::of(10.0),
        system_lifetime: 1,
        size: Spread::of(size),
        start_size_rate: Spread::ZERO,
        size_rate: Spread::ZERO,
        size_rate_damping: Spread::of(1.0),
        alpha: vec![AlphaKey {
            alpha: Spread::of(0.5),
            at: 0,
        }],
        color: vec![ColorKey {
            color: [1.0, 0.0, 0.0],
            at: 0,
        }],
        color_scale: Spread::ZERO,
        burst_delay: Spread::ZERO,
        burst_count: Spread::of(1.0),
        initial_delay: Spread::ZERO,
        velocity: EmissionVelocity::Still,
        volume: EmissionVolume::Point,
        hollow: false,
        slave: None,
        attached: None,
    }
}

#[test]
fn each_group_s_quads_face_the_view_or_lie_on_the_ground_turned_and_tinted() {
    // A smoke of size 1 facing the view at the origin, mixed by its alpha; a scorch of size 2
    // turned a right angle on the ground at (10, 0, 0), of an alpha test.
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let systems = BTreeMap::from([
        (
            name("smoke"),
            system("client/textures/smoke.ktx2", Blend::Alpha, 1.0, 0.0, false),
        ),
        (
            name("scorch"),
            system(
                "client/textures/scorch.ktx2",
                Blend::Mask { cutoff: 0.5 },
                2.0,
                FRAC_PI_2,
                true,
            ),
        ),
    ]);
    let sets = EffectSets {
        systems,
        ..EffectSets::default()
    };
    let book = EffectBook::new(&EffectsFile::new(1.0 / 30.0, 10, sets).unwrap());
    let mut run = ParticleRun::new(1);
    for (text, at) in [("smoke", Vec3::ZERO), ("scorch", Vec3::new(10.0, 0.0, 0.0))] {
        run.start(
            &book,
            book.id(&name(text)).unwrap(),
            Affine3A::from_translation(at),
        );
    }
    run.step(&book, |_, _| 0.0);
    let mut quads = ParticleQuads::default();
    let view = ViewAxes {
        right: Vec3::X,
        up: Vec3::Y,
    };
    quads.build(&book, &run, view);
    // The groups by their systems' names: the scorch's first.
    let [scorch, smoke] = [GroupId::of(0), GroupId::of(1)].map(|id| quads.group(id).clone());
    // The smoke's corners half a unit about it in the view's plane, from its top left
    // counter-clockwise, red at half its alpha, its sRGB shares 1 and 0 the same in linear.
    assert_eq!(
        smoke.positions,
        [
            [-0.5, 0.5, 0.0],
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0]
        ]
    );
    assert_eq!(smoke.uvs, [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
    assert_eq!(smoke.colors, [[1.0, 0.0, 0.0, 0.5]; 4]);
    assert_eq!(smoke.indices, [0, 1, 2, 0, 2, 3]);
    // The scorch's corners a unit about it on the ground, turned a right angle: its top left,
    // (−0.5, 0.5), turns to (−0.5, −0.5), along the ground's `x` and `y`, the engine's `x` and
    // `−z`, so (10 − 1, 0, 1). An alpha test draws its texture untinted.
    let expected = [
        [9.0, 0.0, 1.0],
        [11.0, 0.0, 1.0],
        [11.0, 0.0, -1.0],
        [9.0, 0.0, -1.0],
    ];
    for (corner, expected) in scorch.positions.iter().zip(expected) {
        assert!(
            Vec3::from(*corner).abs_diff_eq(Vec3::from(expected), 1e-6),
            "{corner:?}"
        );
    }
    assert_eq!(scorch.colors, [[1.0; 4]; 4]);
    assert_eq!(scorch.indices, [0, 1, 2, 0, 2, 3]);
}
