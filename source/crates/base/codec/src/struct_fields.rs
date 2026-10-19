use std::cell::Cell;
use std::error::Error;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};

/// The names serde reads a struct's fields by, as its derive or its own impl gives them to the
/// format: what splits one table among the types that each read some of its keys, and what lists
/// a type's fields.
#[derive(Debug)]
pub struct StructFields;

impl StructFields {
    /// The fields `T` reads, in the order it declares them; none for a type that reads no struct.
    pub fn of<'de, T: Deserialize<'de>>() -> &'static [&'static str] {
        let fields = Cell::new(&[][..]);
        let read = T::deserialize(FieldNames(&fields));
        debug_assert!(read.is_err(), "a reader of field names reads no value");
        fields.get()
    }
}

/// A deserializer that reads only the field names of the struct it is asked for.
#[derive(Debug)]
struct FieldNames<'a>(&'a Cell<&'static [&'static str]>);

#[derive(Debug)]
struct NoValue;

impl fmt::Display for NoValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a reader of field names reads no value")
    }
}

impl Error for NoValue {}

impl de::Error for NoValue {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

impl<'de> Deserializer<'de> for FieldNames<'_> {
    type Error = NoValue;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, NoValue> {
        Err(NoValue)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, NoValue> {
        self.0.set(fields);
        Err(NoValue)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum
        identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "only the names of its fields are read")]
    struct Derived {
        first: u8,
        #[serde(rename = "renamed")]
        second: u8,
    }

    #[derive(Debug)]
    struct ByHand;

    impl<'de> Deserialize<'de> for ByHand {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            Derived::deserialize(deserializer)?;
            Ok(Self)
        }
    }

    #[test]
    fn a_struct_gives_the_names_it_reads_and_another_type_none() {
        assert_eq!(StructFields::of::<Derived>(), ["first", "renamed"]);
        assert_eq!(StructFields::of::<ByHand>(), ["first", "renamed"]);
        assert!(StructFields::of::<u8>().is_empty());
    }
}
