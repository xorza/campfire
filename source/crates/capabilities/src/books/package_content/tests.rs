use serde::de::value::Error;
use serde::de::{Deserializer, Error as _, Visitor};
use serde::forward_to_deserialize_any;

use super::*;

/// Reads no value: it keeps the field names a struct's derived `Deserialize` asks for.
#[derive(Debug, Default)]
struct FieldNames(&'static [&'static str]);

impl<'de> Deserializer<'de> for &mut FieldNames {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error::custom("not a struct"))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Error> {
        self.0 = fields;
        Err(Error::custom("the field names are kept"))
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier
        ignored_any
    }
}

#[test]
fn the_keys_are_the_fields() {
    let mut names = FieldNames::default();
    assert!(PackageContent::deserialize(&mut names).is_err());
    assert_eq!(names.0, PackageContent::KEYS);
}
