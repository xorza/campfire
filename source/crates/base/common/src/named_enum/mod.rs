use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, DeserializeSeed, EnumAccess, Error, VariantAccess, Visitor};
use serde::{Deserializer, Serializer};

/// An enum of a fixed set whose every member has one name, as data writes it: the derive
/// `NamedEnum` implements it, naming each member by its identifier in snake case or by its
/// `#[name = "…"]`, so a name never leaves its member.
pub trait NamedEnum: Copy + Eq + 'static {
    /// The enum's name, as serde names the type of its unit variants.
    const TYPE: &'static str;
    /// Every member, in order.
    const ALL: &'static [Self];
    /// Each member's name, in the order of `ALL`.
    const NAMES: &'static [&'static str];

    /// The member's place in `ALL`, its discriminant, as the derive gives it.
    fn index(self) -> usize;

    /// The member at `index` in `ALL`.
    fn at(index: usize) -> Option<Self> {
        const {
            assert!(
                Self::ALL.len() == Self::NAMES.len(),
                "a name for each member"
            );
        };
        Self::ALL.get(index).copied()
    }

    /// The member's name.
    fn name(self) -> &'static str {
        Self::NAMES[self.index()]
    }

    /// The member named `name`.
    fn named(name: &str) -> Option<Self> {
        let at = Self::NAMES.iter().position(|&named| named == name)?;
        Self::at(at)
    }

    /// The member as a unit variant of `TYPE`, by its place and its name, as serde's derive
    /// writes one: a binary format keeps its place, a text format its name.
    fn serialize_member<S: Serializer>(self, serializer: S) -> Result<S::Ok, S::Error> {
        let index = u32::try_from(self.index()).expect("an enum's members fit u32");
        serializer.serialize_unit_variant(Self::TYPE, index, self.name())
    }

    /// The member `deserializer` gives as a unit variant, as `serialize_member` writes it: by its
    /// place, or by the name `name` gives it, so the two never differ. Another name fails, with
    /// every name in the error.
    fn deserialize_member<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_enum(Self::TYPE, Self::NAMES, Members(PhantomData))
    }
}

/// Reads a member of `T` as a unit variant, its identifier by `Members`' own `DeserializeSeed`.
struct Members<T>(PhantomData<T>);

impl<T> fmt::Debug for Members<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Members")
    }
}

impl<'de, T: NamedEnum> Visitor<'de> for Members<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "one of {:?}", T::NAMES)
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<T, A::Error> {
        let (member, variant) = data.variant_seed(self)?;
        variant.unit_variant()?;
        Ok(member)
    }

    fn visit_u64<E: Error>(self, index: u64) -> Result<T, E> {
        usize::try_from(index)
            .ok()
            .and_then(T::at)
            .ok_or_else(|| E::invalid_value(de::Unexpected::Unsigned(index), &self))
    }

    fn visit_str<E: Error>(self, name: &str) -> Result<T, E> {
        T::named(name).ok_or_else(|| E::unknown_variant(name, T::NAMES))
    }

    fn visit_bytes<E: Error>(self, name: &[u8]) -> Result<T, E> {
        let name = str::from_utf8(name)
            .map_err(|_| E::invalid_value(de::Unexpected::Bytes(name), &self))?;
        self.visit_str(name)
    }
}

/// A variant's identifier: its place or its name.
impl<'de, T: NamedEnum> DeserializeSeed<'de> for Members<T> {
    type Value = T;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

#[cfg(test)]
mod tests;
