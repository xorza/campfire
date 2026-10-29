use super::*;

/// Pieces that arrive out of order, overlap, or come again hand on each byte once, in order:
/// "lo" at 3 waits, "hel" at 0 releases "hello", "llo w" at 2 adds " w", and "he" again adds
/// nothing.
#[test]
fn pieces_hand_on_each_byte_once_in_order() {
    let mut stream = CryptoStream::default();
    let mut out = Vec::new();
    stream.push(3, b"lo", &mut out);
    assert!(out.is_empty());
    stream.push(0, b"hel", &mut out);
    assert_eq!(out, b"hello");
    stream.push(2, b"llo w", &mut out);
    assert_eq!(out, b"hello w");
    stream.push(0, b"he", &mut out);
    assert_eq!(out, b"hello w");
    stream.push(MAX_AHEAD + 8, b"far", &mut out);
    assert_eq!(out, b"hello w");
    // From 7 delivered, a piece may end at 7 + 65,536 = 65,543 at most.
    stream.push(MAX_AHEAD + 6, b"ab", &mut out);
    stream.push(MAX_AHEAD + 5, b"ab", &mut out);
    assert_eq!(stream.held, ENTRY_COST + 2);
    stream.held = 0;
    stream.ahead.clear();
    // Pieces of 1,000 bytes at 8, 9, 10, … overlap, and each counts whole with its entry:
    // 65,536 / (1,000 + ENTRY_COST) of them fit. The byte at 7 releases them all.
    let piece = [b'x'; 1_000];
    let fit = usize::try_from(MAX_AHEAD).unwrap() / (1_000 + ENTRY_COST);
    for offset in 8..8 + 66 {
        stream.push(offset, &piece, &mut out);
    }
    assert_eq!(
        [stream.ahead.len(), stream.held],
        [fit, fit * (1_000 + ENTRY_COST)]
    );
    stream.push(7, b"!", &mut out);
    assert_eq!([stream.ahead.len(), stream.held], [0, 0]);
    assert_eq!(out.len(), 8 + 1_000 + fit - 1);
    // Pieces of one byte at distinct offsets fill the bound by their entries: 65,536 /
    // (1 + ENTRY_COST) of them.
    for offset in 1_100..3_100 {
        stream.push(offset, b"y", &mut out);
    }
    assert_eq!(
        stream.ahead.len(),
        usize::try_from(MAX_AHEAD).unwrap() / (1 + ENTRY_COST)
    );
    let mut send = CryptoSend::default();
    assert_eq!([send.take(3), send.take(4), send.take(0)], [0, 3, 7]);
}
