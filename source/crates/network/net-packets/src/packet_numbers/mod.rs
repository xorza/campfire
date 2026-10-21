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
pub(crate) const fn is_in_reach(next: PacketNumber, largest_acked: Option<PacketNumber>) -> bool {
    let limit = match largest_acked {
        Some(acked) => acked.0 + WINDOW / 2 - 2,
        None => WINDOW / 2 - 3,
    };
    next.0 <= limit
}

#[cfg(test)]
mod tests;
