use super::*;

/// RFC 9000 §A.3's example: the largest 0xa82f30ea, and 0x9b32 on the wire, is 0xa82f9b32.
#[test]
fn the_rfc_example_reads_whole() {
    assert_eq!(
        expand(Some(PacketNumber(0xa82f_30ea)), 0x9b32),
        PacketNumber(0xa82f_9b32)
    );
}

/// Each number within 2^15 of the one after the largest reads whole, across a wrap of the low
/// bits both ways, and before any number is known.
#[test]
fn a_number_within_half_the_window_reads_whole() {
    for largest in [0_u64, 1, 0xfffe, 0xffff, 0x1_0000, 0x1_7fff, 0x12_3456] {
        let expected = largest + 1;
        let low = expected.saturating_sub(WINDOW / 2 - 1);
        for number in [low, expected, expected + WINDOW / 2 - 1] {
            let number = PacketNumber(number);
            assert_eq!(
                expand(Some(PacketNumber(largest)), truncate(number)),
                number,
                "largest {largest:#x}, number {:#x}",
                number.0
            );
        }
    }
    for number in [0, 1, WINDOW / 2 - 1] {
        assert_eq!(
            expand(None, truncate(PacketNumber(number))),
            PacketNumber(number)
        );
    }
}

/// A sender reaches 2^15 − 2 numbers past the largest acknowledged, and no further: from
/// nothing acknowledged, 0 to 32,765; with 100 acknowledged, 101 to 32,866. At the limit, the
/// sender still reads the peer's acknowledgement of 100 whole, and one past it would not.
#[test]
fn a_sender_reaches_half_the_window_past_the_largest_acknowledged() {
    assert!(is_in_reach(PacketNumber(32_765), None));
    assert!(!is_in_reach(PacketNumber(32_766), None));
    assert!(is_in_reach(PacketNumber(32_866), Some(PacketNumber(100))));
    assert!(!is_in_reach(PacketNumber(32_867), Some(PacketNumber(100))));
    let acked = PacketNumber(100);
    assert_eq!(expand(Some(PacketNumber(32_866)), truncate(acked)), acked);
    assert_ne!(expand(Some(PacketNumber(32_867)), truncate(acked)), acked);
    assert_eq!(
        expand(Some(acked), truncate(PacketNumber(32_866))),
        PacketNumber(32_866)
    );
}
