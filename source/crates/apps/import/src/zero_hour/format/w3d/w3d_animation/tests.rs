use super::*;
use crate::zero_hour::format::w3d::w3d_animation::samples::{
    bit_channel, channel, compressed, raw, timed,
};
use crate::zero_hour::format::w3d::w3d_chunks::samples::chunk;

fn read(bytes: &[u8]) -> Result<W3dAnimation, W3dError> {
    W3dAnimation::read(W3dChunks::of(bytes).next().unwrap().unwrap())
}

#[test]
fn a_raw_animation_reads_a_key_each_frame_wraps_its_last_and_refuses_what_it_cannot_convert() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let turns = [
        0.0, 0.0, 0.0, 1.0, //
        0.0, 0.0, half, half, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, half, -half,
    ];
    // Pivot 1 moves along x at frames 1 and 2, its first channel replaced by its second, and
    // turns each frame; pivot 2 is hidden at frame 1 alone, shown outside its frames.
    let bytes = raw(
        "Man_Run",
        "Man_SKL",
        4,
        &[
            channel(1, 0, 1, 0, &[9.0, 9.0, 9.0, 9.0]),
            channel(1, 0, 1, 1, &[1.0, 2.0]),
            channel(1, 6, 4, 0, &turns),
            bit_channel(2, 1, &[false, true], true),
        ],
    );
    let animation = read(&bytes).unwrap();
    assert_eq!(
        (animation.name.as_str(), animation.hierarchy.as_str()),
        ("man_skl.man_run", "man_skl")
    );
    assert_eq!((animation.frames, animation.rate), (4, 30));
    assert_eq!(animation.pivots.keys().copied().collect::<Vec<_>>(), [1, 2]);
    let pivot = &animation.pivots[&1];
    let along_x = pivot.moves[0].as_ref().unwrap();
    assert!(pivot.moves[1].is_none() && pivot.moves[2].is_none());
    // A frame outside the channel's moves none; frame 4, past the last, is frame 0's.
    assert_eq!(along_x.key_frames(4), [0, 1, 2, 3, 4]);
    let moves = (0..=4).map(|frame| along_x.move_at(frame, 4));
    assert_eq!(moves.collect::<Vec<_>>(), [0.0, 1.0, 2.0, 0.0, 0.0]);
    let turn = pivot.turn.as_ref().unwrap();
    assert_eq!(turn.turn_at(1, 4), [0.0, 0.0, half, half]);
    assert_eq!(turn.turn_at(4, 4), [0.0, 0.0, 0.0, 1.0]);
    let visibility = animation.pivots[&2].visibility.as_ref().unwrap();
    let shown = (0..4).map(|frame| visibility.shown(frame));
    assert_eq!(shown.collect::<Vec<_>>(), [true, false, true, true]);
    // Hidden at frame 1 alone: one run, 1 to 2.
    assert_eq!(visibility.hidden_runs(4), [[1, 2]]);
    // Shown at frame 2 alone, hidden outside its frame: two runs, 0 to 2 and 3 to 5.
    let gaps = VisibilityChannel {
        first: 2,
        count: 1,
        default: false,
        bits: vec![1],
    };
    assert_eq!(gaps.hidden_runs(5), [[0, 2], [3, 5]]);
    // An animation before version 3.0 counts its pivots from the one past the root.
    let mut old = bytes;
    old[16..20].copy_from_slice(&0x0002_0000_u32.to_le_bytes());
    let keys = read(&old).unwrap().pivots.into_keys().collect::<Vec<_>>();
    assert_eq!(keys, [2, 3]);
    // The floats the exporter wrote past a channel's frames are skipped.
    let mut long = channel(1, 2, 1, 0, &[5.0, 6.0]);
    long.extend(7.0_f32.to_le_bytes());
    long[4] += 4;
    let along_z = read(&raw("a", "b", 2, &[long])).unwrap().pivots[&1].moves[2]
        .clone()
        .unwrap();
    assert_eq!([along_z.move_at(0, 2), along_z.move_at(1, 2)], [5.0, 6.0]);
    // An Euler turn; a move of two floats a key; a last frame before the first; no frame.
    let refused = |channels: &[Vec<u8>], frames| read(&raw("a", "b", frames, channels));
    assert_eq!(
        refused(&[channel(1, 3, 1, 0, &[0.0])], 1),
        Err(W3dError::Channel { kind: 3, len: 1 })
    );
    assert_eq!(
        refused(&[channel(1, 0, 2, 0, &[0.0, 0.0])], 1),
        Err(W3dError::Channel { kind: 0, len: 2 })
    );
    let mut backwards = channel(1, 0, 1, 2, &[0.0]);
    backwards[10..12].copy_from_slice(&1_u16.to_le_bytes());
    assert_eq!(refused(&[backwards], 4), Err(W3dError::Counts));
    assert_eq!(refused(&[], 0), Err(W3dError::Timing));
}

#[test]
fn a_time_coded_animation_is_linear_between_its_keys_and_holds_its_last() {
    let [first, second] = [[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 0.0]];
    let along_y = timed(1, 1, 1, &[(0, &[0.0]), (4, &[2.0]), (6, &[3.0])]);
    let turns = timed(1, 6, 4, &[(0x8000_0000, &first), (5, &second)]);
    let animation = read(&compressed("Man_Die", "Man_SKL", 10, 0, &[along_y, turns])).unwrap();
    assert_eq!(animation.name, "man_skl.man_die");
    let pivot = &animation.pivots[&1];
    let along_y = pivot.moves[1].as_ref().unwrap();
    assert_eq!(along_y.key_frames(10), [0, 4, 6, 10]);
    // Frame 2 is half of 0 to 4: 0 + (2 − 0) · 2 / 4 = 1; frame 5 half of 4 to 6: 2 + (3 − 2) · 1 /
    // 2 = 2.5; past frame 6 the last key holds.
    let moves = [2, 4, 5, 6, 9, 10].map(|frame| along_y.move_at(frame, 10));
    assert_eq!(moves, [1.0, 2.0, 2.5, 3.0, 3.0, 3.0]);
    // The first key's step is never read.
    let turn = pivot.turn.as_ref().unwrap();
    assert_eq!(turn.key_frames(10), [0, 5, 10]);
    assert_eq!(turn.turn_at(0, 10), first);
    assert_eq!(
        [turn.turn_at(5, 10), turn.turn_at(10, 10)],
        [second, second]
    );
    // A step past the first key; keys not from frame 0, not rising, or past the last frame, and
    // none; adaptive delta; a time-coded visibility channel.
    let refused = |flavor, channels: &[Vec<u8>]| read(&compressed("a", "b", 10, flavor, channels));
    let keys = |keys: &[(u32, &[f32])]| vec![timed(1, 0, 1, keys)];
    assert_eq!(
        refused(0, &keys(&[(0, &[0.0]), (0x8000_0003, &[1.0])])),
        Err(W3dError::Step)
    );
    for bad in [
        &[(1, &[0.0][..])][..],
        &[(0, &[0.0][..]), (0, &[1.0][..])][..],
        &[(0, &[0.0][..]), (10, &[1.0][..])][..],
        &[][..],
    ] {
        assert_eq!(refused(0, &keys(bad)), Err(W3dError::TimeCodes), "{bad:?}");
    }
    assert_eq!(refused(1, &[]), Err(W3dError::Flavor(1)));
    assert_eq!(
        refused(0, &[chunk(0x0283, false, &[])]),
        Err(W3dError::TimedVisibility)
    );
}
