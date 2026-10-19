//! A packet number on the wire: its low 16 bits, which the receiver makes whole again from the
//! largest number it knows of that direction (RFC 9000 §17.1 and §A.3).

use campfire_net_payload::PacketNumber;

/// The numbers the low 16 bits tell apart.
const WINDOW: u64 = 1 << 16;

/// The low 16 bits of `number`, which the wire carries.
pub(crate) const fn truncate(number: PacketNumber) -> u16 {
    number.0 as u16
}

/// The whole number whose low 16 bits are `truncated`, nearest the one after `largest`, the
/// largest number of the direction the receiver knows: RFC 9000 §A.3 with 16 bits. A sender
/// keeps each number it sends within 2^15 of the largest the peer acknowledged, so the nearest
/// is the one sent.
pub(crate) const fn expand(largest: Option<PacketNumber>, truncated: u16) -> PacketNumber {
    let expected = match largest {
        Some(largest) => largest.0 + 1,
        None => 0,
    };
    let half = WINDOW / 2;
    let candidate = (expected & !(WINDOW - 1)) | truncated as u64;
    if candidate + half <= expected {
        PacketNumber(candidate + WINDOW)
    } else if candidate > expected + half && candidate >= WINDOW {
        PacketNumber(candidate - WINDOW)
    } else {
        PacketNumber(candidate)
    }
}

/// Whether a sender may send `next` while the peer acknowledged at most `largest_acked`. Both
/// numbers on the wire then read whole: `next` at the peer, whose largest is at least
/// `largest_acked`, needs `next ≤ largest_acked + 1 + 2^15`; and the peer's acknowledgement of
/// `largest_acked`, which the sender reads against its own largest sent, needs
/// `largest sent ≤ largest_acked + 2^15 − 2`. With nothing acknowledged the bound counts from −1.
pub(crate) const fn in_reach(next: PacketNumber, largest_acked: Option<PacketNumber>) -> bool {
    let limit = match largest_acked {
        Some(acked) => acked.0 + WINDOW / 2 - 2,
        None => WINDOW / 2 - 3,
    };
    next.0 <= limit
}

#[cfg(test)]
mod tests {
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
        assert!(in_reach(PacketNumber(32_765), None));
        assert!(!in_reach(PacketNumber(32_766), None));
        assert!(in_reach(PacketNumber(32_866), Some(PacketNumber(100))));
        assert!(!in_reach(PacketNumber(32_867), Some(PacketNumber(100))));
        let acked = PacketNumber(100);
        assert_eq!(expand(Some(PacketNumber(32_866)), truncate(acked)), acked);
        assert_ne!(expand(Some(PacketNumber(32_867)), truncate(acked)), acked);
        assert_eq!(
            expand(Some(acked), truncate(PacketNumber(32_866))),
            PacketNumber(32_866)
        );
    }
}
