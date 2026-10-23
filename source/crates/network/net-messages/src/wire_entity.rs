use bevy_ecs::entity::{Entity, EntityGeneration, EntityIndex};
use campfire_net_bytes::{Reader, SerializationError, ToBytes, VarInt, WriteInteger};

/// An entity in its wire form: its index word, then its generation unless it is the first. Both
/// are usually small, so each goes as a varint. A newtype, as the orphan rule keeps `ToBytes` off
/// Bevy's `Entity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireEntity(pub Entity);

/// The flag in an index word that the entity is of the first generation, which then goes unsent.
const FIRST_GENERATION: u64 = 1;

impl ToBytes for WireEntity {
    fn bytes_len(&self) -> usize {
        let generation = self.0.generation();
        let mut len = VarInt::new(index_word(self.0)).encoded_len();
        if generation != EntityGeneration::FIRST {
            len += VarInt::new(u64::from(generation.to_bits())).encoded_len();
        }
        len
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        let generation = self.0.generation();
        buffer.write_varint(index_word(self.0));
        if generation != EntityGeneration::FIRST {
            buffer.write_varint(u64::from(generation.to_bits()));
        }
    }

    /// An entity of the first generation in its long form, the flag clear and its generation
    /// sent, is refused, as each entity has one encoding.
    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError> {
        let word = buffer.read_varint()?;
        let first = EntityGeneration::FIRST.to_bits();
        let generation = if word & FIRST_GENERATION == 0 {
            let generation = u32::try_from(buffer.read_varint()?)
                .map_err(|_| SerializationError::InvalidValue)?;
            if generation == first {
                return Err(SerializationError::NotCanonical);
            }
            generation
        } else {
            first
        };
        let index = u32::try_from(word >> 1).map_err(|_| SerializationError::InvalidValue)?;
        let index = EntityIndex::from_raw_u32(index).ok_or(SerializationError::InvalidValue)?;
        Ok(Self(Entity::from_index_and_generation(
            index,
            EntityGeneration::from_bits(generation),
        )))
    }
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
        WireEntity(entity).to_bytes(&mut writer);
        let written = writer.len();
        assert_eq!(WireEntity(entity).bytes_len(), written);
        let mut reader = Reader::from(writer.take_written());
        assert_eq!(WireEntity::from_bytes(&mut reader).unwrap().0, entity);
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
        // Index 1 of the first generation as its long form, its word 2 and its generation 0,
        // which the short form `[3]` encodes: refused.
        let mut long = Reader::from(vec![2, 0]);
        assert_eq!(
            WireEntity::from_bytes(&mut long),
            Err(SerializationError::NotCanonical)
        );
    }
}
