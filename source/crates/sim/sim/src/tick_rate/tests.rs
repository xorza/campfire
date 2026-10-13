use super::*;

#[test]
fn milliseconds_round_up_to_whole_ticks() {
    // At 30 Hz 250 ms is 7.5 ticks, up to 8; at 20 Hz exactly 5; at 60 Hz exactly 15. 1 ms
    // is under a tick at every rate, up to 1; 2000 ms is exactly 40, 60 and 120.
    let cases = [
        (20, [(0, 0), (1, 1), (250, 5), (2000, 40)]),
        (30, [(0, 0), (1, 1), (250, 8), (2000, 60)]),
        (60, [(0, 0), (1, 1), (250, 15), (2000, 120)]),
    ];
    for (hz, at_rate) in cases {
        let rate = TickRate::new(NonZeroU32::new(hz).unwrap());
        for (ms, ticks) in at_rate {
            assert_eq!(
                rate.ticks(ms),
                Some(Ticks::new(ticks)),
                "{ms} ms at {hz} Hz"
            );
        }
        assert_eq!(rate.ticks(u64::MAX), None);
        // A duration lasts a tick at the least; a window too long to count is every tick.
        assert_eq!(rate.duration(0), Some(Ticks::ONE));
        assert_eq!(rate.duration(250), rate.ticks(250));
        assert_eq!(rate.duration(u64::MAX), None);
        assert_eq!(rate.window(0), Ticks::new(0));
        assert_eq!(rate.window(2000), rate.ticks(2000).unwrap());
        assert_eq!(rate.window(u64::MAX), Ticks::new(u64::MAX));
    }
    // 10⁹ ns ÷ 30 = 33 333 333.3, down to 33 333 333; ÷ 20 = 50 000 000 exactly.
    let length = |hz| TickRate::new(NonZeroU32::new(hz).unwrap()).length();
    assert_eq!(
        (length(30), length(20)),
        (Duration::from_nanos(33_333_333), Duration::from_millis(50))
    );
}
