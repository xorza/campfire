use serde::de::value::{Error as ValueError, StrDeserializer};

use super::*;

#[test]
fn a_block_reads_from_its_name_and_from_no_other() {
    for block in Block::ALL {
        let name = StrDeserializer::<ValueError>::new(block.name());
        assert_eq!(Block::deserialize(name), Ok(block));
    }
    let other = StrDeserializer::<ValueError>::new("Move");
    let refused = Block::deserialize(other).unwrap_err().to_string();
    assert_eq!(
        refused,
        "unknown variant `Move`, expected one of `move`, `attack`, `cast`, `use`, `produce`, `target`, `damage`"
    );
}
