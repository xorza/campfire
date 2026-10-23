use campfire_log::internals::round_trip;

use super::*;

#[test]
fn a_frame_of_several_ticks_delays_only_the_inputs_stamped_within_them() {
    let tick = Tick::new;
    // A frame that started with tick 50 next and ended with 56 ran 50 to 55; one that
    // ended with 52 ran 50 and 51; one that ended with 51 or 50 ran one tick or none.
    let caught_up = TicksCaughtUp::of(tick(50), tick(56)).unwrap();
    assert_eq!(
        caught_up,
        TicksCaughtUp {
            first: tick(50),
            last: tick(55)
        }
    );
    assert_eq!(
        TicksCaughtUp::of(tick(50), tick(52)),
        Some(TicksCaughtUp {
            first: tick(50),
            last: tick(51)
        })
    );
    assert_eq!(TicksCaughtUp::of(tick(50), tick(51)), None);
    assert_eq!(TicksCaughtUp::of(tick(50), tick(50)), None);
    // Stamped 50 and 55, taking effect in 56, they waited; stamped 49 or 56, or taking
    // effect in another tick, they did not.
    assert!(caught_up.delayed(tick(50), tick(56)));
    assert!(caught_up.delayed(tick(55), tick(56)));
    assert!(!caught_up.delayed(tick(49), tick(56)));
    assert!(!caught_up.delayed(tick(56), tick(56)));
    assert!(!caught_up.delayed(tick(50), tick(57)));
    round_trip(&caught_up);
}
