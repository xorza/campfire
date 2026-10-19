use super::*;
use crate::packet::{Packet, PacketCompressionInfo};
use crate::packet_kind::PacketKind;
use crate::packet_message::PacketMessage;
use crate::tick_encoder::LONG_TICK_LEN;
#[cfg(feature = "compression_lz4")]
use bytes::Bytes;
use bytes::BytesMut;
use campfire_net_payload::OutgoingPayload;
#[cfg(feature = "compression_lz4")]
use lz4_flex::block::CompressTable;

/// The bytes before a test packet's body: the packet session's headroom and a long tick.
const HEADER_LEN: usize = OutgoingPayload::HEADROOM + LONG_TICK_LEN;

/// Compresses the packet's body in place when its compressed form is smaller and fits `room`.
fn compress_packet(
    packet: &mut Packet,
    compression: PacketCompression,
    scratch: &mut CompressionScratch,
    output: &mut Vec<u8>,
) -> Result<CompressionOutcome, PacketError> {
    let outcome = compression.evaluate(&packet.payload[HEADER_LEN..], scratch, output)?;
    let CompressionOutcome::Compressed {
        original_len,
        compressed_len,
    } = outcome
    else {
        return Ok(outcome);
    };
    packet.kind = PacketKind::DataCompressed;
    packet.payload.truncate(HEADER_LEN);
    packet.payload.extend_from_slice(&output[..compressed_len]);
    packet.compression = Some(PacketCompressionInfo {
        original_len,
        compressed_len,
    });
    Ok(outcome)
}

/// Decompresses `compressed` alone.
#[cfg(feature = "compression_lz4")]
pub(crate) fn decompress(compressed: &[u8], setup: CompressionSetup) -> Result<Bytes, PacketError> {
    let mut out = Writer::default();
    decompress_payload(compressed, setup, &mut out)?;
    Ok(out.take_written())
}

fn packet_with_body(body: &[u8]) -> Packet {
    let mut payload = BytesMut::with_capacity(HEADER_LEN + body.len());
    payload.resize(OutgoingPayload::HEADROOM, 0);
    payload.extend_from_slice(&[0x80, 0, 0, 0, 0]);
    payload.extend_from_slice(body);
    Packet {
        payload,
        kind: PacketKind::Data,
        messages: Vec::<PacketMessage>::new(),
        compression: None,
        compression_abandoned: false,
    }
}

const fn room(setup: CompressionSetup) -> PacketCompression {
    PacketCompression { setup, room: 1200 }
}

#[test]
fn a_disabled_compression_leaves_the_packet_as_it_is() {
    let mut packet = packet_with_body(&[1, 2, 3, 4]);
    let original = packet.payload.as_ref().to_vec();
    let mut scratch = CompressionScratch::default();
    let mut output = Vec::new();

    let outcome = compress_packet(
        &mut packet,
        room(CompressionSetup::DISABLED),
        &mut scratch,
        &mut output,
    )
    .unwrap();

    assert_eq!(outcome, CompressionOutcome::Disabled);
    assert_eq!(packet.payload.as_ref(), original.as_slice());
}

#[cfg(feature = "compression_lz4")]
#[test]
fn an_lz4_compression_changes_the_packet_kind_and_keeps_the_body() {
    let original_body = vec![7u8; 512];
    let mut packet = packet_with_body(&original_body);
    let original_len = packet.payload.len();
    let setup = CompressionSetup {
        min_payload_len: 0,
        ..CompressionSetup::LZ4
    };
    let original_capacity = packet.payload.capacity();
    let original_pointer = packet.payload.as_ref().as_ptr();
    let mut scratch = CompressionScratch::default();
    let mut output = Vec::new();

    let outcome = compress_packet(&mut packet, room(setup), &mut scratch, &mut output).unwrap();

    assert!(matches!(outcome, CompressionOutcome::Compressed { .. }));
    assert!(packet.payload.len() < original_len);
    assert_eq!(packet.payload.capacity(), original_capacity);
    assert_eq!(packet.payload.as_ref().as_ptr(), original_pointer);
    assert_eq!(packet.kind, PacketKind::DataCompressed);
    let decompressed = decompress(&packet.payload.as_ref()[HEADER_LEN..], setup).unwrap();
    assert_eq!(decompressed, original_body);
}

#[cfg(feature = "compression_lz4")]
#[test]
fn an_lz4_compression_that_saves_nothing_keeps_the_packet_uncompressed() {
    let original_body = *b"small payload that should expand";
    let mut packet = packet_with_body(&original_body);
    let original = packet.payload.as_ref().to_vec();
    let setup = CompressionSetup {
        min_payload_len: 0,
        ..CompressionSetup::LZ4
    };
    let mut scratch = CompressionScratch::default();
    let mut output = Vec::new();

    let outcome = compress_packet(&mut packet, room(setup), &mut scratch, &mut output).unwrap();

    assert!(matches!(outcome, CompressionOutcome::NotSmaller { .. }));
    assert_eq!(packet.payload.as_ref(), original.as_slice());
}

#[cfg(feature = "compression_lz4")]
#[test]
/// The size prefix of a 32-byte payload is checked against the limit before decompressing:
/// a limit of exactly 32 takes it, and 31 refuses it with both numbers.
fn an_lz4_decompression_keeps_to_its_length_bound() {
    let mut output = Vec::new();
    let compressed =
        CompressionScratch::compress_lz4(&[3u8; 32], &mut output, &mut CompressTable::default())
            .unwrap();
    let limited = |max_decompressed_payload_len| CompressionSetup {
        max_decompressed_payload_len,
        ..CompressionSetup::LZ4
    };

    assert_eq!(decompress(compressed, limited(32)).unwrap(), vec![3u8; 32]);
    assert!(matches!(
        decompress(compressed, limited(31)).unwrap_err(),
        PacketError::DecompressedPayloadTooLarge {
            actual: 32,
            limit: 31
        }
    ));
}

#[cfg(feature = "compression_lz4")]
#[test]
fn an_lz4_decompression_refuses_a_malformed_payload() {
    let err = decompress(&[1, 2, 3], CompressionSetup::LZ4).unwrap_err();

    assert!(matches!(err, PacketError::DecompressionFailed));
}

/// Two payloads decompressed into one writer share its chunk: the 32 bytes of the second start
/// where the 40 bytes of the first end, so a packet decompresses without an allocation of its
/// own.
#[cfg(feature = "compression_lz4")]
#[test]
fn payloads_decompress_into_one_writer_chunk() {
    let mut output = Vec::new();
    let mut table = CompressTable::default();
    let first = CompressionScratch::compress_lz4(&[1u8; 40], &mut output, &mut table)
        .unwrap()
        .to_vec();
    let second = CompressionScratch::compress_lz4(&[2u8; 32], &mut output, &mut table)
        .unwrap()
        .to_vec();
    let mut out = Writer::default();
    decompress_payload(&first, CompressionSetup::LZ4, &mut out).unwrap();
    let first = out.take_written();
    decompress_payload(&second, CompressionSetup::LZ4, &mut out).unwrap();
    let second = out.take_written();
    assert_eq!(&*first, &[1u8; 40][..]);
    assert_eq!(&*second, &[2u8; 32][..]);
    assert_eq!(second.as_ptr(), first.as_ptr().wrapping_add(40));
}
