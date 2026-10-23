use std::any::type_name;
use std::collections::BTreeMap;
use std::fmt;

use bevy_ecs::world::World;
use campfire_codec::Binary;
use campfire_common::{EnumAll, StateHash, Tick};
use campfire_math::SplitMix64;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, Deserializer, EnumAccess, IntoDeserializer, MapAccess,
    SeqAccess, VariantAccess, Visitor,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::sim_state::{SimComponent, SimResource};
use crate::stable_id::StableId;
use crate::stable_id::entity_index::EntityIndex;
use crate::state_registry::state_delta::StateDelta;
use crate::state_registry::{ENTITIES, StateRegistry};

/// The most a layout fingerprint draws each value again when its type's decode refuses it. The
/// fingerprints pinned in the tests hold the draws this number makes, so it changes only with
/// them.
const LAYOUT_TRIES: usize = 16;

/// The most a scramble draws its value again when its type's decode refuses it. A unit's pools,
/// eight optional meters, decode only when every meter that is there holds an amount from 0 to a
/// positive maximum: as an option is none one draw in four, that is about one draw in 6,000, and
/// a failed draw costs a few numbers, so a scramble draws long enough to reach one most times.
const SCRAMBLE_TRIES: usize = 8192;

/// The fewest values of a type a layout fingerprint encodes. Its numbers, options and lengths are
/// drawn at random: where each draw meets an option, 32 draws miss its none, or a length, with
/// odds of (3/4)³², about 1 in 10⁴.
const LAYOUT_DRAWS: usize = 32;

/// The most values of a type a layout fingerprint encodes, while a variant it met has no draw:
/// 4⁶, as six enums of four variants, each in the last variant of the one before, take. Past it,
/// a variant is out of the draws' reach.
const LAYOUT_MAX_DRAWS: usize = 1 << 12;

/// Draws values of any state type through its own decode, as a snapshot could hold them: each
/// number at an edge, 0, 1, 2, `Tick::LIMIT` and one past it, the largest and one below it, or a
/// small or any value; each list, map and string short; each choice of an enum's variants, of an
/// option and of a bool even. The draws follow a seed, so a run repeats.
#[derive(Debug)]
pub struct Draws {
    words: SplitMix64,
    /// What a layout's draws met; none for draws that only make values.
    shape: Option<Shape>,
}

/// What the draws of a layout met. Its trace holds each read the decodes asked for, in order,
/// a refused decode's too: the layout itself, each read a tag, a tuple's length after its tag as
/// 4 bytes, and a struct's fields' or an enum's variants' names after its tag, which give a
/// field's or a variant's place its meaning, each name ended by 0 and the list by 1. It counts
/// how many times each enum was met, whose variants the draws take in turn. An enum is known by
/// its name and variants, so two instances of one generic enum share their turns.
#[derive(Debug)]
struct Shape {
    trace: Vec<u8>,
    met: BTreeMap<(&'static str, &'static [&'static str]), usize>,
}

/// A kind of read of a decode, as a layout's trace tags it. A unit, a unit struct and a newtype
/// struct read no byte, so none has a tag; a `str` reads as a `String`, and bytes as a byte
/// buffer.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum Read {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Bool,
    Char,
    Float,
    String,
    Bytes,
    Option,
    Seq,
    Tuple,
    Map,
    Struct,
    Enum,
    /// A value of no fixed type: untyped, ignored, or a field's name.
    Untyped,
}

impl Shape {
    fn read(&mut self, read: Read) {
        self.trace.push(read as u8);
    }

    fn tuple(&mut self, len: usize) {
        self.read(Read::Tuple);
        let len = u32::try_from(len).expect("a tuple's length fits u32");
        self.trace.extend_from_slice(&len.to_le_bytes());
    }

    fn names(&mut self, read: Read, names: &[&str]) {
        self.read(read);
        for name in names {
            self.trace.extend_from_slice(name.as_bytes());
            self.trace.push(0);
        }
        self.trace.push(1);
    }

    /// The variant of the enum `name` of `variants` to draw: each in turn.
    fn variant(&mut self, name: &'static str, variants: &'static [&'static str]) -> usize {
        let met = self.met.entry((name, variants)).or_default();
        let index = *met % variants.len();
        *met += 1;
        index
    }

    /// Whether each variant of each enum met had a draw.
    fn drew_every_variant(&self) -> bool {
        self.met
            .iter()
            .all(|(&(_, variants), &met)| met >= variants.len())
    }
}

impl Draws {
    pub const fn new(seed: u64) -> Self {
        Self {
            words: SplitMix64::new(seed),
            shape: None,
        }
    }

    /// Draws of `seed` that note what they meet, for a layout.
    const fn shaped(seed: u64) -> Self {
        Self {
            words: SplitMix64::new(seed),
            shape: Some(Shape {
                trace: Vec::new(),
                met: BTreeMap::new(),
            }),
        }
    }

    fn read(&mut self, read: Read) {
        if let Some(shape) = &mut self.shape {
            shape.read(read);
        }
    }

    fn tuple(&mut self, len: usize) {
        if let Some(shape) = &mut self.shape {
            shape.tuple(len);
        }
    }

    fn names(&mut self, read: Read, names: &[&str]) {
        if let Some(shape) = &mut self.shape {
            shape.names(read, names);
        }
    }

    /// A draw below `count`.
    const fn below(&mut self, count: u64) -> u64 {
        self.words.next_u64() % count
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
            _ => self.words.next_u64() & max,
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

    /// A value of `T` its decode accepts, drawn again up to `tries` times; `None` when every draw
    /// fails.
    fn value<T: for<'de> Deserialize<'de>>(&mut self, tries: usize) -> Option<T> {
        (0..tries).find_map(|_| T::deserialize(&mut *self).ok())
    }

    /// Puts a drawn value of `C` in place of one entity's, of those that hold one, or, when none
    /// does, on a drawn entity of the match, so a type the match holds nowhere is drawn too; false
    /// when the match has no entity, or no draw decodes.
    pub(crate) fn scramble_component<C: SimComponent>(&mut self, world: &mut World) -> bool {
        let (holders, others): (Vec<_>, Vec<_>) = world
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .partition(|&entity| world.get::<C>(entity).is_some());
        let chosen = if holders.is_empty() {
            self.pick(&others)
        } else {
            self.pick(&holders)
        };
        let Some(&entity) = chosen else {
            return false;
        };
        let Some(value) = self.value::<C>(SCRAMBLE_TRIES) else {
            return false;
        };
        world.entity_mut(entity).insert(value);
        true
    }

    /// Spawns an entity of a drawn stable id that holds no part, as a snapshot's entity list
    /// could name one; false when the match has an entity of that id, or no draw decodes.
    pub(crate) fn scramble_entities(&mut self, world: &mut World) -> bool {
        let Some(id) = self.value::<StableId>(SCRAMBLE_TRIES) else {
            return false;
        };
        if world.resource::<EntityIndex>().get(id).is_some() {
            return false;
        }
        world.spawn(id);
        true
    }

    /// Changes `C`, as `mutation` says, on one entity of those that hold one: where it sits,
    /// moved or copied to one of those that hold none, or one of its `Some`s; false when no entity
    /// holds one, none lacks it for a move or a copy, or the value holds no `Some` to drop.
    pub(crate) fn mutate_component<C: SimComponent>(
        &mut self,
        world: &mut World,
        mutation: Mutation,
    ) -> bool {
        let (holders, others): (Vec<_>, Vec<_>) = world
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .partition(|&entity| world.get::<C>(entity).is_some());
        let Some(&holder) = self.pick(&holders) else {
            return false;
        };
        match mutation {
            Mutation::Remove => {
                world.entity_mut(holder).remove::<C>();
                return true;
            }
            Mutation::DropOption => {
                let held = world.get::<C>(holder).expect("a holder holds its value");
                let Some(value) = self.without_option(held) else {
                    return false;
                };
                world.entity_mut(holder).insert(value);
                return true;
            }
            Mutation::Move | Mutation::Copy => {}
        }
        let Some(&other) = self.pick(&others) else {
            return false;
        };
        let value = match mutation {
            Mutation::Move => world.entity_mut(holder).take::<C>(),
            Mutation::Copy => {
                let held = world.get::<C>(holder).expect("a holder holds its value");
                Binary::decode::<C>(&Binary::encode(held)).ok()
            }
            Mutation::Remove | Mutation::DropOption => unreachable!("each returned above"),
        };
        world
            .entity_mut(other)
            .insert(value.expect("a held value takes and copies"));
        true
    }

    /// Changes `R` in the match, as `mutation` says: a resource can lose its value, or one of its
    /// `Some`s, as no entity holds it; false when it has none, or nothing to change so.
    pub(crate) fn mutate_resource<R: SimResource>(
        &mut self,
        world: &mut World,
        mutation: Mutation,
    ) -> bool {
        match mutation {
            Mutation::Remove => world.remove_resource::<R>().is_some(),
            Mutation::DropOption => {
                let Some(held) = world.get_resource::<R>() else {
                    return false;
                };
                let Some(value) = self.without_option(held) else {
                    return false;
                };
                world.insert_resource(value);
                true
            }
            Mutation::Move | Mutation::Copy => false,
        }
    }

    /// `value` with one of its `Some`s, drawn, as `None`; none when it holds no `Some`, or its
    /// type refuses the change, as a value that must hold one.
    fn without_option<T: Serialize + DeserializeOwned>(&mut self, value: &T) -> Option<T> {
        let count = u64::try_from(Binary::options(value)).expect("a count fits u64");
        if count == 0 {
            return None;
        }
        let at = usize::try_from(self.below(count)).expect("a drawn place");
        let bytes = Binary::encode_without_option(value, at).expect("the drawn some is held");
        Binary::decode::<T>(&bytes).ok()
    }

    /// One of `items`, drawn; none when it is empty.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let count = u64::try_from(items.len()).expect("a count fits u64");
        (count > 0).then(|| &items[usize::try_from(self.below(count)).expect("a place")])
    }

    /// Writes into `out` the encodings of values of `T` the draws make, a byte that says none
    /// for each draw no value decodes from, then the trace of what the decodes read: at least
    /// `LAYOUT_DRAWS` values, and more until each variant of each enum met had a draw.
    pub(crate) fn layout_of<T: Serialize + for<'de> Deserialize<'de>>(
        &mut self,
        out: &mut Vec<u8>,
    ) {
        let mut drawn = 0;
        while drawn < LAYOUT_DRAWS || !self.shape().drew_every_variant() {
            assert!(
                drawn < LAYOUT_MAX_DRAWS,
                "a variant of {} is out of the draws' reach",
                type_name::<T>()
            );
            match self.value::<T>(LAYOUT_TRIES) {
                Some(value) => {
                    out.push(1);
                    Binary::encode_into(&value, out);
                }
                None => out.push(0),
            }
            drawn += 1;
        }
        out.extend_from_slice(&self.shape().trace);
    }

    const fn shape(&self) -> &Shape {
        self.shape
            .as_ref()
            .expect("a layout's draws note their shape")
    }

    /// Puts a drawn value of `R` in place of the match's, or in the match that has none; false
    /// when no draw decodes.
    pub(crate) fn scramble_resource<R: SimResource>(&mut self, world: &mut World) -> bool {
        let Some(value) = self.value::<R>(SCRAMBLE_TRIES) else {
            return false;
        };
        world.insert_resource(value);
        true
    }
}

/// A change of where a state type's value sits, which a snapshot could hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum Mutation {
    /// One holder loses its value.
    Remove,
    /// One holder's value moves to an entity that holds none.
    Move,
    /// One holder's value is copied to an entity that holds none.
    Copy,
    /// One holder's value, or a resource, with one of its `Some`s, drawn, as `None`: a value of
    /// its type that a draw rarely makes, as every other part stays as it was.
    DropOption,
}

/// Why a draw does not make a value: a type the state does not hold, or a refusal of its decode.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct DrawError(String);

impl de::Error for DrawError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
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

/// The drawn variant of an enum.
#[derive(Debug)]
struct Variant<'a> {
    draws: &'a mut Draws,
    index: usize,
}

impl<'de, 'a> EnumAccess<'de> for Variant<'a> {
    type Error = DrawError;
    type Variant = &'a mut Draws;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, &'a mut Draws), DrawError> {
        let index = u32::try_from(self.index).expect("variants fit u32");
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
        self.tuple(len);
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
        self.names(Read::Struct, fields);
        self.tuple(fields.len());
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
        self.read(Read::U8);
        visitor.visit_u8(self.unsigned_of())
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::U16);
        visitor.visit_u16(self.unsigned_of())
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::U32);
        visitor.visit_u32(self.unsigned_of())
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::U64);
        visitor.visit_u64(self.unsigned_of())
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::I8);
        visitor.visit_i8(self.signed_of())
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::I16);
        visitor.visit_i16(self.signed_of())
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::I32);
        visitor.visit_i32(self.signed_of())
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::I64);
        visitor.visit_i64(self.signed_of())
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::U128);
        visitor.visit_u128(u128::from(self.unsigned(64)))
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::I128);
        visitor.visit_i128(i128::from(self.signed(64)))
    }

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        self.read(Read::Untyped);
        Err(DrawError("no untyped value".to_owned()))
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::Bool);
        visitor.visit_bool(self.below(2) == 1)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        self.read(Read::Float);
        Err(DrawError("no float".to_owned()))
    }

    fn deserialize_f64<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        self.read(Read::Float);
        Err(DrawError("no float".to_owned()))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::Char);
        visitor.visit_char('a')
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::String);
        let names = ["", "a", "phase", "x_1"];
        let at = usize::try_from(self.below(4)).expect("a name's place");
        visitor.visit_string(names[at].to_owned())
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::Bytes);
        let len = self.len();
        let bytes = (0..len)
            .map(|_| self.words.next_u64().to_le_bytes()[0])
            .collect();
        visitor.visit_byte_buf(bytes)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DrawError> {
        self.read(Read::Option);
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
        self.read(Read::Seq);
        let left = self.len();
        visitor.visit_seq(Drawn { draws: self, left })
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        self.tuple(len);
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
        self.read(Read::Map);
        let left = self.len();
        visitor.visit_map(Drawn { draws: self, left })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        self.names(Read::Struct, fields);
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DrawError> {
        let index = if let Some(shape) = &mut self.shape {
            shape.variant(name, variants)
        } else {
            let count = u64::try_from(variants.len()).expect("variants fit u64");
            usize::try_from(self.below(count)).expect("variants fit usize")
        };
        self.names(Read::Enum, variants);
        visitor.visit_enum(Variant { draws: self, index })
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        self.read(Read::Untyped);
        Err(DrawError("no field names".to_owned()))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, DrawError> {
        self.read(Read::Untyped);
        Err(DrawError("no untyped value".to_owned()))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl StateRegistry {
    /// Each registered type's layout fingerprint, in their order: the digest of what its own
    /// decode reads, in order, as values are drawn through it from a seed of its name, every
    /// variant of each enum it meets among them, and of the encodings of the values it accepts.
    /// A type's fingerprint moves with no other type's, and when what its decode reads, its
    /// encoding or its decode's checks change, the order or a name of its fields or variants
    /// among them. What its decode reads moves it also when the decode refuses every draw. Two
    /// fields of one type in a tuple, which have no names, can change places with no change. The entity list's holds no draw: the snapshot
    /// golden pins it.
    pub fn layout(&self) -> Vec<(&'static str, StateHash)> {
        let mut bytes = Vec::new();
        self.entries
            .iter()
            .map(|entry| {
                let seed = *Self::digest(entry.name.as_bytes()).as_bytes();
                let seed = u64::from_le_bytes(seed[..8].try_into().expect("8 of 32 bytes"));
                bytes.clear();
                (entry.layout)(&mut Draws::shaped(seed), &mut bytes);
                (entry.name, Self::digest(&bytes))
            })
            .collect()
    }

    /// The name of each registered type, in their order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.iter().map(|entry| entry.name)
    }

    /// Changes where the state type `name` sits in `world`, as `mutation` says, by `draws`;
    /// false when the type has nothing to change so.
    pub fn mutate(
        &self,
        world: &mut World,
        name: &str,
        draws: &mut Draws,
        mutation: Mutation,
    ) -> bool {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .expect("a registered type");
        (entry.mutate)(draws, world, mutation)
    }

    /// Puts a drawn value of the type `name` in `world`, in place of one holder's or where none
    /// is; false when no draw decodes.
    pub fn scramble(&self, world: &mut World, name: &str, draws: &mut Draws) -> bool {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .expect("a registered type");
        (entry.scramble)(draws, world)
    }

    /// The names of the types of which `delta`, the changes of `world` since its last copy,
    /// holds a value, a removal or an id gained or lost, the entity list's: each type whose
    /// section differs from the one a copy at once after gives, as a type that holds no change,
    /// a resource that is absent among them, writes the same bytes each copy. Starts recording
    /// again, as `changes` does.
    pub fn changed_types(&self, world: &mut World, delta: &StateDelta) -> Vec<&'static str> {
        let mut unchanged = StateDelta::default();
        self.changes(world, &mut unchanged);
        let entities = !(delta.lost.is_empty() && delta.gained.is_empty());
        let entries = self.entries.iter().enumerate();
        entries
            .filter(|&(at, entry)| {
                let ids = entry.name == ENTITIES && entities;
                ids || delta.section(at) != unchanged.section(at)
            })
            .map(|(_, entry)| entry.name)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five enums, each of whose last variant holds the next: at random, a draw reaches `Fifth`'s
    /// last variant once in 4⁵ = 1,024.
    #[derive(Debug, Serialize, Deserialize)]
    enum First {
        A,
        B,
        C,
        D(Second),
    }

    #[derive(Debug, Serialize, Deserialize)]
    enum Second {
        A,
        B,
        C,
        D(Third),
    }

    #[derive(Debug, Serialize, Deserialize)]
    enum Third {
        A,
        B,
        C,
        D(Fourth),
    }

    #[derive(Debug, Serialize, Deserialize)]
    enum Fourth {
        A,
        B,
        C,
        D(Fifth),
    }

    #[derive(Debug, Serialize, Deserialize)]
    enum Fifth {
        A,
        B,
        C,
        D(u32),
    }

    #[test]
    fn a_layouts_draws_take_each_variant_of_each_enum_they_meet_in_turn() {
        // `First` is met in each draw and takes `D` in each 4th, so `Second` is met in each 4th
        // draw, `Third` in each 16th, `Fourth` in each 64th and `Fifth` in each 256th: `Fifth`
        // has its 4th variant at draw 4 × 256 = 1,024, where the draws stop, whatever the seed.
        for seed in [0, 1] {
            let mut draws = Draws::shaped(seed);
            draws.layout_of::<First>(&mut Vec::new());
            let met: Vec<_> = draws
                .shape()
                .met
                .iter()
                .map(|(&(name, _), &met)| (name, met))
                .collect();
            let expected = [
                ("Fifth", 4),
                ("First", 1_024),
                ("Fourth", 16),
                ("Second", 256),
                ("Third", 64),
            ];
            assert_eq!(met, expected, "seed {seed}");
        }
    }
}
