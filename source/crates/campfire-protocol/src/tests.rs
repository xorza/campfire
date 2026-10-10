use std::array;
use std::fmt::{Debug, Display, Write};
use std::str::FromStr;

use campfire_common::{Binary, Fingerprint, NotHex, SegmentSeed, StateHash};
use serde::Serialize;

use crate::connect::ConnectChallenge;
use crate::connect::certificate_hash::CertificateHash;
use crate::delegation::delegation_id::DelegationId;
use crate::delegation::seed_contribution::SeedContribution;
use crate::input_hash::InputHash;
use crate::seed_commitment::SeedCommitment;
use crate::server_seed::ServerSeed;
use crate::session_id::SessionId;
use crate::snapshot_fingerprint::SnapshotFingerprint;

/// 32 bytes, each other than its neighbours, and with a leading zero digit in hex.
fn sample() -> [u8; 32] {
    array::from_fn(|at| u8::try_from(at).unwrap().wrapping_mul(37).wrapping_add(5))
}

/// `sample()` in lowercase hex: two digits a byte, a leading zero kept.
fn hex() -> String {
    sample().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").unwrap();
        text
    })
}

/// Whether `value` holds `sample()` and encodes as the bare 32 bytes, as every 32-byte value does.
fn encodes_its_bytes<T: Serialize>(value: &T, bytes: &[u8; 32]) {
    assert_eq!(bytes, &sample());
    assert_eq!(Binary::encode(value), Binary::encode(&sample()));
}

/// Whether a public `value` of `sample()` writes as their hex.
fn writes_hex<T: Display + Serialize>(value: &T, bytes: &[u8; 32]) {
    encodes_its_bytes(value, bytes);
    assert_eq!(value.to_string(), hex());
}

/// Whether a public `value` of `sample()` that text names reads back from its hex alone.
fn reads_hex<T: Display + Serialize + FromStr<Err = NotHex> + PartialEq + Debug>(
    value: &T,
    bytes: &[u8; 32],
) {
    writes_hex(value, bytes);
    assert_eq!(hex().parse::<T>().as_ref(), Ok(value));
    assert_eq!(hex().to_uppercase().parse::<T>(), Err(NotHex));
    assert_eq!(hex()[..62].parse::<T>(), Err(NotHex));
}

#[test]
fn each_32_byte_value_encodes_its_bytes_and_a_public_one_writes_them_as_hex() {
    // `sample()[0]` is 5, written `05`: the hex keeps its leading zero.
    assert!(hex().starts_with("05"));
    assert_eq!(hex().len(), 64);

    let sample = sample();
    let state = StateHash::new(sample);
    reads_hex(&state, state.as_bytes());
    let session = SessionId::new(sample);
    reads_hex(&session, session.as_bytes());
    let certificate = CertificateHash::new(sample);
    reads_hex(&certificate, certificate.as_bytes());
    let contribution = SeedContribution::new(sample);
    reads_hex(&contribution, contribution.as_bytes());

    let fingerprint = Fingerprint::new(sample);
    writes_hex(&fingerprint, fingerprint.as_bytes());
    let snapshot = SnapshotFingerprint::new(sample);
    writes_hex(&snapshot, snapshot.as_bytes());
    let input = InputHash::new(sample);
    writes_hex(&input, input.as_bytes());
    let commitment = SeedCommitment::new(sample);
    writes_hex(&commitment, commitment.as_bytes());
    let challenge = ConnectChallenge::new(sample);
    writes_hex(&challenge, challenge.as_bytes());
    let delegation = DelegationId::new(sample);
    writes_hex(&delegation, delegation.as_bytes());

    // A secret has no text form; its encoding is its bytes as well.
    let server = ServerSeed::new(sample);
    encodes_its_bytes(&server, server.as_bytes());
    let segment = SegmentSeed::new(sample);
    encodes_its_bytes(&segment, segment.as_bytes());
}
