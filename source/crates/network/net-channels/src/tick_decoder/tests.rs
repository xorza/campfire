use super::*;
use crate::tick_encoder::TickEncoder;

fn read(decoder: &mut TickDecoder, bytes: &[u8]) -> Result<NetTick, ReceiveError> {
    Ok(decoder.read(&mut Reader::from(bytes.to_vec()))?.tick())
}

/// A short tick is the nearest to the largest read, across a wrap of its 15 bits each way:
/// from 0x7FF0, 0x0005 is 0x8005; from 0x8005, 0x7FF0 is 0x7FF0 again.
#[test]
fn a_short_tick_is_the_nearest_to_the_largest_read() {
    assert_eq!(nearest(0x7FF0, 0x0005), 0x8005);
    assert_eq!(nearest(0x8005, 0x7FF0), 0x7FF0);
    assert_eq!(nearest(100, 101), 101);
    assert_eq!(nearest(0, 0x7FFF), 0x7FFF);
    assert_eq!(nearest(u32::MAX, 0x0000), u32::MAX - 0x7FFF);
}

/// What the encoder writes, the decoder reads back: a whole tick, then short ones near it,
/// over the wrap of the low 15 bits, each with its overstep, at the wire's 1/256.
#[test]
fn the_decoder_reads_what_the_encoder_writes() {
    let mut encoder = TickEncoder::default();
    let mut decoder = TickDecoder::default();
    for (tick, overstep) in [(0x7FF0_u32, 0), (0x7FF8, 1), (0x8003, 128), (0x8010, 255)] {
        let at = TickInstant::from_tick_and_overstep(NetTick(tick), Overstep::from_u8(overstep));
        let encoded = encoder.encode(at);
        let mut reader = Reader::from(encoded.as_bytes().to_vec());
        assert_eq!(decoder.read(&mut reader).unwrap(), at);
        encoder.on_sent(NetTick(tick));
        encoder.on_delivered(NetTick(tick));
    }
}

/// A short tick before a whole one, and a long form with another bit set, break the protocol.
#[test]
fn a_tick_with_no_reference_or_a_bad_mark_is_refused() {
    let mut decoder = TickDecoder::default();
    assert!(matches!(
        read(&mut decoder, &[0, 1, 0]),
        Err(ReceiveError::MalformedTick)
    ));
    assert!(matches!(
        read(&mut decoder, &[0x81, 0, 0, 0, 1, 0]),
        Err(ReceiveError::MalformedTick)
    ));
    assert_eq!(
        read(&mut decoder, &[0x80, 0, 0, 0, 1, 0]).unwrap(),
        NetTick(1)
    );
}
