use std::error::Error;
use std::fmt;

use bevy_ecs::world::World;
use campfire_common::Tick;
use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

use crate::entity_index::EntityIndex;
use crate::sim_state::{SimComponent, SimResource};
use crate::state_registry::StateRegistry;

/// The most each value is drawn again when its type's decode refuses it.
const TRIES: usize = 16;

/// Draws values of any state type through its own decode, as a snapshot could hold them: each
/// number at an edge, 0, 1, 2, `Tick::LIMIT` and one past it, the largest and one below it, or a
/// small or any value; each list, map and string short; each choice of an enum's variants, of an
/// option and of a bool even. The draws follow a seed, so a run repeats.
#[derive(Debug)]
pub struct Draws {
    state: u64,
}

impl Draws {
    pub const fn new(seed: u64) -> Draws {
        Draws { state: seed }
    }

    /// The next `SplitMix64` draw.
    const fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }

    /// A draw below `count`.
    const fn below(&mut self, count: u64) -> u64 {
        self.next() % count
    }

    /// A number of `bits` bits, at an edge most of the time.
    const fn unsigned(&mut self, bits: u32) -> u64 {
        let max = u64::MAX >> (u64::BITS - bits);
        let limit = if Tick::LIMIT.get() < max {
            Tick::LIMIT.get()
        } else {
            max
        };
        match self.below(10) {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => limit,
            4 => limit.saturating_add(1) & max,
            5 => max,
            6 => max - 1,
            7 => self.below(100),
            _ => self.next() & max,
        }
    }

    /// A signed number of `bits` bits, at an edge most of the time.
    const fn signed(&mut self, bits: u32) -> i64 {
        let max = i64::MAX >> (i64::BITS - bits);
        match self.below(8) {
            0 => 0,
            1 => -1,
            2 => max,
            3 => -max - 1,
            4 => -self.below(100).cast_signed(),
            _ => {
                let drawn = self.unsigned(bits).cast_signed();
                drawn << (i64::BITS - bits) >> (i64::BITS - bits)
            }
        }
    }

    /// An unsigned number of `T`'s bits.
    fn unsigned_of<T: TryFrom<u64>>(&mut self) -> T {
        let bits = u32::try_from(size_of::<T>() * 8).expect("a number's bits");
        T::try_from(self.unsigned(bits))
            .ok()
            .expect("a draw within its bits")
    }

    /// A signed number of `T`'s bits.
    fn signed_of<T: TryFrom<i64>>(&mut self) -> T {
        let bits = u32::try_from(size_of::<T>() * 8).expect("a number's bits");
        T::try_from(self.signed(bits))
            .ok()
            .expect("a draw within its bits")
    }

    /// A short length.
    fn len(&mut self) -> usize {
        usize::try_from(self.below(4)).expect("a short length")
    }

    /// A value of `T` its decode accepts, drawn again up to `TRIES` times; `None` when every draw
    /// fails.
    fn value<T: for<'de> Deserialize<'de>>(&mut self) -> Option<T> {
        (0..TRIES).find_map(|_| T::deserialize(&mut *self).ok())
    }

    /// Puts a drawn value of `C` in place of one entity's, of those that hold one; false when none
    /// does, or no draw decodes.
    pub(crate) fn scramble_component<C: SimComponent>(&mut self, world: &mut World) -> bool {
        let holders: Vec<_> = world
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .filter(|&entity| world.get::<C>(entity).is_some())
            .collect();
        if holders.is_empty() {
            return false;
        }
        let count = u64::try_from(holders.len()).expect("holders fit u64");
        let entity = holders[usize::try_from(self.below(count)).expect("a holder's place")];
        let Some(value) = self.value::<C>() else {
            return false;
        };
        world.entity_mut(entity).insert(value);
        true
    }

    /// Puts a drawn value of `R` in place of the match's; false when it has none, or no draw
    /// decodes.
    pub(crate) fn scramble_resource<R: SimResource>(&mut self, world: &mut World) -> bool {
        if !world.contains_resource::<R>() {
            return false;
        }
        let Some(value) = self.value::<R>() else {
            return false;
        };
        world.insert_resource(value);
        true
    }
}

/// Why a draw does not make a value: a type the state does not hold, or a refusal of its decode.
#[derive(Debug)]
pub struct DrawError(String);

impl fmt::Display for DrawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DrawError {}

impl de::Error for DrawError {
    fn custom<T: fmt::Display>(message: T) -> DrawError {
        DrawError(message.to_string())
    }
}

/// A list of `left` drawn elements: a sequence, a tuple, a struct's fields or a map's entries.
#[derive(Debug)]
struct Drawn<'a> {
    draws: &'a mut Draws,
    left: usize,
}

impl<'de> SeqAccess<'de> for Drawn<'_> {
    type Error = DrawError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, DrawError> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(&mut *self.draws).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}

impl<'de> MapAccess<'de> for Drawn<'_> {
    type Error = DrawError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, DrawError> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(&mut *self.draws).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, DrawError> {
        seed.deserialize(&mut *self.draws)
    }
}

/// A drawn variant of an enum of `count` variants.
#[derive(Debug)]
struct Variant<'a> {
    draws: &'a mut Draws,
    count: usize,
}

impl<'de, 'a> EnumAccess<'de> for Variant<'a> {
    type Error = DrawError;
    type Variant = &'a mut Draws;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, &'a mut Draws), DrawError> {
        let count = u64::try_from(self.count).expect("variants fit u64");
        let index = u32::try_from(self.draws.below(count)).expect("variants fit u32");
        let variant = seed.deserialize(index.into_deserializer())?;
        Ok((variant, self.draws))
    }
}

impl<'de> VariantAccess<'de> for &mut Draws {
    type Error = DrawError;

    fn unit_variant(self) -> Result<(), DrawError> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, DrawError> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_seq(Drawn {
            draws: self,
            left: len,
        })
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        visitor.visit_seq(Drawn {
            draws: self,
            left: fields.len(),
        })
    }
}

/// The state is a non-self-describing format's: each value asks for its own shape, which the
/// draw follows; a float, which no state holds, or an untyped value, is refused.
impl<'de> Deserializer<'de> for &mut Draws {
    type Error = DrawError;

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_u8(self.unsigned_of())
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_u16(self.unsigned_of())
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_u32(self.unsigned_of())
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_u64(self.unsigned_of())
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_i8(self.signed_of())
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_i16(self.signed_of())
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_i32(self.signed_of())
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_i64(self.signed_of())
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_u128(u128::from(self.unsigned(64)))
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_i128(i128::from(self.signed(64)))
    }

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        Err(DrawError("no untyped value".to_owned()))
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_bool(self.below(2) == 1)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        Err(DrawError("no float".to_owned()))
    }

    fn deserialize_f64<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        Err(DrawError("no float".to_owned()))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_char('a')
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        let names = ["", "a", "phase", "x_1"];
        let at = usize::try_from(self.below(4)).expect("a name's place");
        visitor.visit_string(names[at].to_owned())
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        let len = self.len();
        let bytes = (0..len).map(|_| self.next().to_le_bytes()[0]).collect();
        visitor.visit_byte_buf(bytes)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        if self.below(4) == 0 {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        let left = self.len();
        visitor.visit_seq(Drawn { draws: self, left })
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        visitor.visit_seq(Drawn {
            draws: self,
            left: len,
        })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        let left = self.len();
        visitor.visit_map(Drawn { draws: self, left })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        visitor.visit_enum(Variant {
            draws: self,
            count: variants.len(),
        })
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        Err(DrawError("no field names".to_owned()))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        Err(DrawError("no untyped value".to_owned()))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl StateRegistry {
    /// The name of each registered type, in their order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.iter().map(|entry| entry.name)
    }

    /// Puts a drawn value of the type `name` in place of one holder's in `world`; false when
    /// nothing holds one, or no draw decodes.
    pub fn scramble(&self, world: &mut World, name: &str, draws: &mut Draws) -> bool {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .expect("a registered type");
        (entry.scramble)(draws, world)
    }
}
