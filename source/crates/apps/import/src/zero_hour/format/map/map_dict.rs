//! A map's dictionary, as `DataChunkInput::readDict` reads it: a count, then each key, named by
//! the map's table, with its type, and its value.

use std::collections::BTreeMap;

use crate::zero_hour::format::map::chunk_reader::ChunkReader;
use crate::zero_hour::format::map::error::MapReadError;

/// A dictionary value the import reads, or one it skips.
#[derive(Debug)]
pub(crate) enum DictValue<'a> {
    Int(i32),
    Text(&'a [u8]),
    Other,
}

/// Reads the dictionary at the front of `body`, its keys named by `names`, and calls `entry` with
/// each key and its value, in the order the map gives them, so a key set twice ends at its last.
pub(crate) fn read<'a>(
    body: &mut ChunkReader<'a>,
    names: &BTreeMap<u32, &[u8]>,
    mut entry: impl FnMut(&[u8], DictValue<'a>),
) -> Result<(), MapReadError> {
    for _ in 0..body.cursor.u16()? {
        let key_and_type = body.cursor.i32()?;
        let key_id = (key_and_type >> 8).cast_unsigned();
        let key = *names
            .get(&key_id)
            .ok_or(MapReadError::UnknownName(key_id))?;
        let value = match key_and_type.to_le_bytes()[0] {
            0 => body.cursor.take(1).map(|_| DictValue::Other)?,
            1 => DictValue::Int(body.cursor.i32()?),
            2 => body.cursor.take(4).map(|_| DictValue::Other)?,
            3 => DictValue::Text(body.text()?),
            4 => {
                let len = body.cursor.u16()?;
                body.cursor
                    .take(usize::from(len) * 2)
                    .map(|_| DictValue::Other)?
            }
            other => return Err(MapReadError::DictType(other)),
        };
        entry(key, value);
    }
    Ok(())
}
