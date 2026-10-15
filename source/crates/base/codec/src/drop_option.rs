use std::fmt;

use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};

/// The `Some`s an encode meets, in order, and the one it writes as `None`.
#[derive(Debug)]
pub(crate) struct OptionCount {
    pub(crate) seen: usize,
    pub(crate) dropped: usize,
}

/// A postcard serializer that writes the `Some` `count` names as `None`, and every other value as
/// postcard does. Postcard's wire format, stable since 1.0, writes an option as a tag byte, 0 or
/// 1, then a `Some`'s value, and a compound value as its prefix alone, its length or its variant,
/// then its parts: so each value writes through the inner serializer, and each part through this
/// one again.
pub(crate) struct DropOption<'a, S> {
    pub(crate) inner: &'a mut S,
    pub(crate) count: &'a mut OptionCount,
}

impl<S> fmt::Debug for DropOption<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DropOption")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

impl<S> DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    fn part<T: Serialize + ?Sized>(&mut self, value: &T) -> postcard::Result<()> {
        value.serialize(DropOption {
            inner: &mut *self.inner,
            count: &mut *self.count,
        })
    }
}

/// Forwards each method that writes a whole value to the inner serializer.
macro_rules! forward {
    ($($method:ident($($arg:ident: $ty:ty),*);)*) => {
        $(fn $method(self, $($arg: $ty),*) -> postcard::Result<()> {
            ser::Serializer::$method(&mut *self.inner, $($arg),*)
        })*
    };
}

impl<S> ser::Serializer for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    forward! {
        serialize_bool(v: bool);
        serialize_i8(v: i8);
        serialize_i16(v: i16);
        serialize_i32(v: i32);
        serialize_i64(v: i64);
        serialize_i128(v: i128);
        serialize_u8(v: u8);
        serialize_u16(v: u16);
        serialize_u32(v: u32);
        serialize_u64(v: u64);
        serialize_u128(v: u128);
        serialize_f32(v: f32);
        serialize_f64(v: f64);
        serialize_char(v: char);
        serialize_str(v: &str);
        serialize_bytes(v: &[u8]);
        serialize_none();
        serialize_unit();
        serialize_unit_struct(name: &'static str);
        serialize_unit_variant(name: &'static str, index: u32, variant: &'static str);
    }

    fn serialize_some<T: Serialize + ?Sized>(mut self, value: &T) -> postcard::Result<()> {
        let at = self.count.seen;
        self.count.seen += 1;
        if at == self.count.dropped {
            return ser::Serializer::serialize_none(&mut *self.inner);
        }
        ser::Serializer::serialize_u8(&mut *self.inner, 1)?;
        self.part(value)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        mut self,
        _: &'static str,
        value: &T,
    ) -> postcard::Result<()> {
        self.part(value)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        mut self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> postcard::Result<()> {
        ser::Serializer::serialize_unit_variant(&mut *self.inner, name, index, variant)?;
        self.part(value)
    }

    fn serialize_seq(self, len: Option<usize>) -> postcard::Result<Self> {
        ser::Serializer::serialize_seq(&mut *self.inner, len)?;
        Ok(self)
    }

    fn serialize_tuple(self, len: usize) -> postcard::Result<Self> {
        ser::Serializer::serialize_tuple(&mut *self.inner, len)?;
        Ok(self)
    }

    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> postcard::Result<Self> {
        ser::Serializer::serialize_tuple_struct(&mut *self.inner, name, len)?;
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> postcard::Result<Self> {
        ser::Serializer::serialize_tuple_variant(&mut *self.inner, name, index, variant, len)?;
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> postcard::Result<Self> {
        ser::Serializer::serialize_map(&mut *self.inner, len)?;
        Ok(self)
    }

    fn serialize_struct(self, name: &'static str, len: usize) -> postcard::Result<Self> {
        ser::Serializer::serialize_struct(&mut *self.inner, name, len)?;
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> postcard::Result<Self> {
        ser::Serializer::serialize_struct_variant(&mut *self.inner, name, index, variant, len)?;
        Ok(self)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl<S> SerializeSeq for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}

impl<S> SerializeTuple for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}

impl<S> SerializeTupleStruct for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}

impl<S> SerializeTupleVariant for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}

impl<S> SerializeMap for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> postcard::Result<()> {
        self.part(key)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}

impl<S> SerializeStruct for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _: &'static str,
        value: &T,
    ) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}

impl<S> SerializeStructVariant for DropOption<'_, S>
where
    for<'b> &'b mut S: ser::Serializer<Ok = (), Error = postcard::Error>,
{
    type Ok = ();
    type Error = postcard::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _: &'static str,
        value: &T,
    ) -> postcard::Result<()> {
        self.part(value)
    }

    fn end(self) -> postcard::Result<()> {
        Ok(())
    }
}
