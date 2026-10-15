use campfire_codec::Binary;

use super::*;

/// A meaning of the tests, whose values are SHA-256 of content.
#[derive(Debug)]
enum Probe {}

impl Meaning for Probe {
    const NAME: &'static str = "Probe";
}

impl ContentMeaning for Probe {}

#[test]
fn a_value_writes_as_hex_reads_back_and_encodes_as_its_bytes() {
    let mut bytes = [0; 32];
    bytes[0] = 0xAB;
    bytes[31] = 0x05;
    let value = Value32::<Probe>::new(bytes);
    let written = format!("ab{}05", "00".repeat(30));
    assert_eq!(value.to_string(), written);
    assert_eq!(format!("{value:?}"), format!("Probe({written})"));
    assert_eq!(Value32::<Probe>::of_hex(&written), value);
    assert_eq!(written.parse(), Ok(value));
    assert_eq!(
        written.to_uppercase().parse::<Value32<Probe>>(),
        Err(NotHex)
    );
    assert_eq!((value.get(), *value.as_bytes()), (bytes, bytes));
    let encoded = Binary::encode(&value);
    assert_eq!(encoded, Binary::encode(&bytes));
    assert_eq!(Binary::decode::<Value32<Probe>>(&encoded).unwrap(), value);
    // Values order by their bytes; a content meaning's is the SHA-256 of its bytes.
    assert!(Value32::<Probe>::new([0; 32]) < value);
    assert_eq!(
        Value32::<Probe>::of(b"abc").to_string(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
