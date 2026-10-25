use super::*;

/// A clip of 11 frames, its last 10, played by `mode` from `start`, moved by each of `steps`:
/// its frame after each.
fn frames(mode: AnimationMode, start: ClipStart, steps: &[f32]) -> Vec<f32> {
    let mut clock = ClipClock::start(11, mode, start);
    steps
        .iter()
        .map(|&by| {
            clock.advance(by);
            clock.frame()
        })
        .collect()
}

#[test]
fn each_mode_moves_its_frame_as_w3d_does_and_once_completes_at_its_end() {
    use AnimationMode::{Loop, LoopBackwards, Manual, Once, OnceBackwards, PingPong};
    // Once: 4, then 4 + 7 = 11, held at 10, complete there.
    assert_eq!(frames(Once, ClipStart::Natural, &[4.0, 7.0]), [4.0, 10.0]);
    let mut once = ClipClock::start(11, Once, ClipStart::Natural);
    once.advance(9.5);
    assert!(!once.is_complete());
    once.advance(1.0);
    assert!(once.is_complete());
    // A loop wraps at its last frame: 12 − 10 = 2; 25 − 10 = 15, still past it, so 0.
    assert_eq!(frames(Loop, ClipStart::Natural, &[12.0]), [2.0]);
    assert_eq!(frames(Loop, ClipStart::Natural, &[25.0]), [0.0]);
    // Backwards from its last frame: 10 − 3 = 7; 7 − 9 = −2, plus 10, 8.
    assert_eq!(
        frames(LoopBackwards, ClipStart::Natural, &[3.0, 9.0]),
        [7.0, 8.0]
    );
    // Once backwards: 10 − 12 = −2, held at 0, complete there.
    let mut back = ClipClock::start(11, OnceBackwards, ClipStart::Natural);
    back.advance(12.0);
    assert_eq!((back.frame(), back.is_complete()), (0.0, true));
    // A ping-pong turns at its last frame: 12 is 2 past, so 8, backwards; then 8 − 10 = −2, so
    // 2, forwards. A turn within a frame of its end holds at it: 10.5 turns to 9.5, so 10.
    assert_eq!(
        frames(PingPong, ClipStart::Natural, &[12.0, 10.0, 1.0]),
        [8.0, 2.0, 3.0]
    );
    assert_eq!(frames(PingPong, ClipStart::Natural, &[10.5]), [10.0]);
    // A manual clip holds its start; no loop or manual clip completes.
    assert_eq!(frames(Manual, ClipStart::Last, &[5.0]), [10.0]);
    for mode in [Manual, Loop, PingPong, LoopBackwards] {
        assert!(
            !ClipClock::start(11, mode, ClipStart::Last).is_complete(),
            "{mode:?}"
        );
    }
    // A clip of one frame stands at it, its share 0.
    let mut single = ClipClock::start(1, Loop, ClipStart::Natural);
    single.advance(3.0);
    assert_eq!((single.frame(), single.share()), (0.0, 0.0));
}

#[test]
fn a_clip_starts_where_its_start_says_within_its_frames() {
    let start = |mode, start| ClipClock::start(11, mode, start).frame();
    // Its mode's own: the first forwards, the last backwards; the first and the last whatever
    // its mode; a frame of its own within the clip.
    assert_eq!(start(AnimationMode::Loop, ClipStart::Natural), 0.0);
    assert_eq!(
        start(AnimationMode::OnceBackwards, ClipStart::Natural),
        10.0
    );
    assert_eq!(start(AnimationMode::OnceBackwards, ClipStart::First), 0.0);
    assert_eq!(start(AnimationMode::Loop, ClipStart::Last), 10.0);
    assert_eq!(start(AnimationMode::Loop, ClipStart::At(4)), 4.0);
    assert_eq!(start(AnimationMode::Loop, ClipStart::At(20)), 10.0);
    // A kept share of half: 0.5 · 11 − 1 = 4.5, truncated to 4; of none, −1, within the clip 0.
    assert_eq!(start(AnimationMode::Loop, ClipStart::Kept(0.5)), 4.0);
    assert_eq!(start(AnimationMode::Loop, ClipStart::Kept(0.0)), 0.0);
    // The share it stands at: 5 of its last 10, a half.
    let mut clock = ClipClock::start(11, AnimationMode::Loop, ClipStart::Natural);
    clock.advance(5.0);
    assert_eq!(clock.share(), 0.5);
}
