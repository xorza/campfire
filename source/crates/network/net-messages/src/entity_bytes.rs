//! The wire form of an entity: its index word, then its generation unless it is the first. Both
//! are usually small, so each goes as a varint.

use bevy_ecs::entity::{Entity, EntityGeneration, EntityIndex};
use campfire_net_bytes::{ReadVarInt, Reader, SerializationError, WriteInteger, varint};

/// The flag in an index word that the entity is of the first generation, which then goes unsent.
const FIRST_GENERATION: u64 = 1;

/// The bytes `entity` takes on the wire.
pub fn len(entity: Entity) -> usize {
    let generation = entity.generation();
    let mut len = varint::len(index_word(entity));
    if generation != EntityGeneration::FIRST {
        len += varint::len(u64::from(generation.to_bits()));
    }
    len
}

/// Writes `entity` to `buffer`.
pub fn write(entity: Entity, buffer: &mut impl WriteInteger) {
    let generation = entity.generation();
    buffer.write_varint(index_word(entity));
    if generation != EntityGeneration::FIRST {
        buffer.write_varint(u64::from(generation.to_bits()));
    }
}

/// Reads an entity from `buffer`.
pub fn read(buffer: &mut Reader) -> Result<Entity, SerializationError> {
    let word = buffer.read_varint()?;
    let generation = if word & FIRST_GENERATION == 0 {
        u32::try_from(buffer.read_varint()?).map_err(|_| SerializationError::InvalidValue)?
    } else {
        0
    };
    let index = u32::try_from(word >> 1).map_err(|_| SerializationError::InvalidValue)?;
    let index = EntityIndex::from_raw_u32(index).ok_or(SerializationError::InvalidValue)?;
    Ok(Entity::from_index_and_generation(
        index,
        EntityGeneration::from_bits(generation),
    ))
}

/// The index above the first-generation flag, in the low bit where a varint keeps it short.
fn index_word(entity: Entity) -> u64 {
    let flag = if entity.generation() == EntityGeneration::FIRST {
        FIRST_GENERATION
    } else {
        0
    };
    (u64::from(entity.index_u32()) << 1) | flag
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_bytes::Writer;

    fn round_trip(entity: Entity) -> usize {
        let mut writer = Writer::with_capacity(100);
        write(entity, &mut writer);
        let written = writer.len();
        assert_eq!(len(entity), written);
        let mut reader = Reader::from(writer.take_written());
        assert_eq!(read(&mut reader).unwrap(), entity);
        written
    }

    /// An entity of the first generation goes as its index word alone, 1 byte for index 1; a
    /// later generation adds it, 2 bytes for generation 1. Every bit of the generation goes on the
    /// wire, the highest too: index 10 at generation 2^31 is the word 10 << 1 = 20, 1 byte as at
    /// most 63, then the generation, 8 bytes as above 2^30 - 1.
    #[test]
    fn an_entity_round_trips_in_its_varints() {
        let entity = |index, generation| {
            Entity::from_index_and_generation(
                EntityIndex::from_raw_u32(index).unwrap(),
                EntityGeneration::from_bits(generation),
            )
        };
        assert_eq!(round_trip(entity(1, 0)), 1);
        assert_eq!(round_trip(entity(1, 1)), 2);
        assert_eq!(round_trip(entity(10, 1 << 31)), 9);
    }
}
