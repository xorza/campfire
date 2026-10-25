use gltf_json::Root;

use super::*;
use crate::zero_hour::format::w3d::w3d_animation::samples::{
    bit_channel, channel, compressed, raw, timed,
};
use crate::zero_hour::format::w3d::w3d_chunks::W3dChunks;
use crate::zero_hour::install::samples::FixtureInstall;

/// A man's hierarchy: a root; a pelvis 1 up; a head 0.5 up on it, a half turn about `z`; and a
/// gun 1 along `x` on the pelvis.
fn man() -> Hierarchy {
    let pivot = |name: &str, parent, translation, rotation| Pivot {
        name: name.to_owned(),
        parent,
        translation,
        rotation,
    };
    let identity = [0.0, 0.0, 0.0, 1.0];
    Hierarchy {
        name: "man_skl".to_owned(),
        pivots: vec![
            pivot("roottransform", None, [0.0; 3], identity),
            pivot("pelvis", Some(0), [0.0, 0.0, 1.0], identity),
            pivot("head", Some(1), [0.0, 0.0, 0.5], [0.0, 0.0, 1.0, 0.0]),
            pivot("gun", Some(1), [1.0, 0.0, 0.0], identity),
        ],
    }
}

fn glb(bytes: &[u8]) -> GlbBuilder {
    let chunk = W3dChunks::of(bytes).next().unwrap().unwrap();
    let animation = W3dAnimation::read(chunk).unwrap();
    AnimationBuilder::new(&animation).build(&man())
}

/// Each channel's node and property, its sampler's input and its output's floats.
fn tracks(glb: &GlbBuilder) -> Vec<(usize, Property, usize, Vec<f32>)> {
    let animation = &glb.document.animations[0];
    animation
        .channels
        .iter()
        .map(|channel| {
            let sampler = &animation.samplers[channel.sampler.value()];
            let Checked::Valid(property) = channel.target.path else {
                panic!("a valid property");
            };
            (
                channel.target.node.value(),
                property,
                sampler.input.value(),
                glb.floats_of(sampler.output),
            )
        })
        .collect()
}

fn extras(document: &Root) -> AnimationExtras {
    let extensions = document.animations[0].extensions.as_ref().unwrap();
    Json::convert(&extensions.others[AnimationExtras::EXTENSION]).unwrap()
}

#[test]
fn a_raw_animation_keys_each_frame_on_its_pivots_and_its_visibility_in_its_extras() {
    // The pelvis moves along `x` 1 and 2 at frames 1 and 2; the head moves 1 along its parent's
    // `x` each frame and turns a half turn about `x` at frames 1 and 3; the gun is hidden at
    // frames 1 and 2.
    let half_x = [1.0, 0.0, 0.0, 0.0];
    let identity = [0.0, 0.0, 0.0, 1.0];
    let turns = [identity, half_x, identity, half_x].concat();
    let glb = glb(&raw(
        "Man_Wave",
        "Man_SKL",
        4,
        &[
            channel(1, 0, 1, 1, &[1.0, 2.0]),
            channel(2, 0, 1, 0, &[1.0; 4]),
            channel(2, 6, 4, 0, &turns),
            bit_channel(3, 1, &[false, false], true),
        ],
    ));
    let document = &glb.document;
    let names: Vec<&str> = document
        .nodes
        .iter()
        .map(|node| node.name.as_deref().unwrap())
        .collect();
    assert_eq!(names, ["roottransform", "pelvis", "head", "gun"]);
    assert_eq!(
        document.animations[0].name.as_deref(),
        Some("man_skl.man_wave")
    );
    // Two key times, each written once: each frame 0 to 4, and the first and the last.
    let each = (0..=4_u8)
        .map(|frame| f32::from(frame) / 30.0)
        .collect::<Vec<_>>();
    let [dense, ends] = [0, 1].map(|at| document.animations[0].samplers[at].input.value());
    assert_eq!(
        glb.floats_of(Index::new(u32::try_from(dense).unwrap())),
        each
    );
    assert_eq!(
        glb.floats_of(Index::new(u32::try_from(ends).unwrap())),
        [0.0, 4.0 / 30.0]
    );
    // On glTF's axes, `(x, y, z)` as `(x, z, −y)`, a turn's axis likewise. The pelvis: its pose
    // 1 up, then its move, (x, 0, 1) as (x, 1, 0), none outside its frames and none at frame 4,
    // which is frame 0's. The head: its pose, then its move turned by its half turn about `z`,
    // (0, 0, 0.5) + (−1, 0, 0) as (−1, 0.5, 0); its turn the half turn about `z` and the
    // channel's, `(0, 0, 1, 0) · (1, 0, 0, 0)`, a half turn about `y`, `(0, 1, 0, 0)`, as
    // `(0, 0, −1, 0)`, else its own half turn, as `(0, 1, 0, 0)`. The gun stands at its pose.
    let pelvis_moves = [
        0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 2.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0,
    ];
    let own = [0.0, 1.0, 0.0, 0.0];
    let turned = [0.0, 0.0, -1.0, 0.0];
    assert_eq!(
        tracks(&glb),
        [
            (1, Property::Translation, dense, pelvis_moves.to_vec()),
            (1, Property::Rotation, ends, identity.repeat(2)),
            (2, Property::Translation, dense, [-1.0, 0.5, 0.0].repeat(5)),
            (
                2,
                Property::Rotation,
                dense,
                [own, turned, own, turned, own].concat()
            ),
            (3, Property::Translation, ends, [1.0, 0.0, 0.0].repeat(2)),
            (3, Property::Rotation, ends, identity.repeat(2)),
        ]
    );
    assert_eq!(
        extras(document),
        AnimationExtras {
            frames: 4,
            rate: 30,
            hidden: vec![HiddenFrames {
                node: 3,
                runs: vec![[1, 3]],
            }],
        }
    );
    assert_eq!(document.extensions_used, [AnimationExtras::EXTENSION]);
}

#[test]
fn a_time_coded_animation_keys_the_frames_of_its_channels_keys() {
    // The pelvis moves along `y` to 2 at frame 4 and 3 at frame 6, and along `z` to 2.5 at
    // frame 5: one track keyed at each frame either keys, and at frame 10, past the last. Along
    // `y`, frame 5 is 2.5, half of 4 to 6; along `z`, frame 4 is 2.5 · 4 / 5 = 2, and 2.5 is held
    // past frame 5.
    let along_y = timed(1, 1, 1, &[(0, &[0.0]), (4, &[2.0]), (6, &[3.0])]);
    let along_z = timed(1, 2, 1, &[(0, &[0.0]), (5, &[2.5])]);
    let glb = glb(&compressed(
        "Man_Die",
        "Man_SKL",
        10,
        0,
        &[along_y, along_z],
    ));
    let tracks = tracks(&glb);
    let (node, property, input, moves) = &tracks[0];
    assert_eq!((*node, *property), (1, Property::Translation));
    let times = [0_u8, 4, 5, 6, 10].map(|frame| f32::from(frame) / 30.0);
    assert_eq!(
        glb.floats_of(Index::new(u32::try_from(*input).unwrap())),
        times
    );
    // The pelvis 1 up, then the move (0, y, z): (0, y, 1 + z) as (0, 1 + z, −y).
    assert_eq!(
        *moves,
        [
            0.0, 1.0, 0.0, 0.0, 3.0, -2.0, 0.0, 3.5, -2.5, 0.0, 3.5, -3.0, 0.0, 3.5, -3.0
        ]
    );
    // No frame hides a pivot.
    assert!(extras(&glb.document).hidden.is_empty());
}

#[test]
fn an_animation_is_found_by_its_name_and_dropped_without_a_hierarchy_or_a_pivot_to_move() {
    let FixtureInstall {
        test_dir: _dir,
        install,
    } = FixtureInstall::new();
    let mut assets = ModelAssets::new(&install).unwrap();
    let mut cause = |name| match ConvertedAnimation::of(name, &mut assets).unwrap() {
        ConvertedAnimation::Glb(_) => None,
        ConvertedAnimation::Dropped(cause) => Some(cause),
    };
    // Its file holds an animation of its full name, of either case, which converts once.
    assert_eq!(cause("Other.Tank_Aim"), Some(AnimationDropCause::Missing));
    assert_eq!(cause("Tank.Tank_Aim"), None);
    assert_eq!(cause("tank.tank_aim"), Some(AnimationDropCause::Missing));
    // A name of no `.`; a file the install lacks.
    assert_eq!(cause("tank_aim"), Some(AnimationDropCause::Missing));
    assert_eq!(cause("tank.tank_gone"), Some(AnimationDropCause::Missing));
    // A hierarchy no file holds; one in its own file of a root alone.
    assert_eq!(
        cause("ghost.ghost_walk"),
        Some(AnimationDropCause::NoHierarchy)
    );
    assert_eq!(cause("pole.pole_sway"), Some(AnimationDropCause::NoPivot));
}
