use crate::message_error::MessageError;
use crate::receive_map_view::ReceiveMapView;
use crate::send_map_view::SendMapView;
use bytes::Buf;
use campfire_codec::{Binary, Sink};
use campfire_net_bytes::Reader;
use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::Writer;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::any::Any;
use std::sync::Arc;

/// The serialization fns of one type, erased so that a registry holds those of every type.
#[derive(Clone, Debug)]
pub(crate) struct ErasedSerializeFns {
    /// The [`TypedSerializeFns`] of the type.
    fns: Arc<dyn Any + Send + Sync>,
    /// Whether the context fns map the entities the type holds.
    pub(crate) maps_entities: bool,
}

/// The four fns of one type that [`ErasedSerializeFns`] erases.
#[derive(Debug)]
struct TypedSerializeFns<M> {
    serialize: CxSerializeFns<M>,
    deserialize: CxDeserializeFns<M>,
}

/// The fns that serialize a type: its own, and the one that runs it with the link's entity map.
pub(crate) struct CxSerializeFns<M> {
    serialize: SerializeFn<M>,
    cx_serialize: CxSerializeFn<M>,
}

// Manual impls: a derive would bound `M`, which only appears in fn pointer types.
impl<M> Clone for CxSerializeFns<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M> Copy for CxSerializeFns<M> {}

impl<M> core::fmt::Debug for CxSerializeFns<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CxSerializeFns").finish_non_exhaustive()
    }
}

impl<M> CxSerializeFns<M> {
    /// `serialize`, with no entity to map.
    pub(crate) fn new(serialize: SerializeFn<M>) -> Self {
        Self {
            serialize,
            cx_serialize: default_cx_serialize::<M>,
        }
    }
}

/// The fns that deserialize a type: its own, and the one that runs it with the link's entity
/// map.
pub(crate) struct CxDeserializeFns<M> {
    deserialize: DeserializeFn<M>,
    cx_deserialize: CxDeserializeFn<M>,
}

// Manual impls: a derive would bound `M`, which only appears in fn pointer types.
impl<M> Clone for CxDeserializeFns<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M> Copy for CxDeserializeFns<M> {}

impl<M> core::fmt::Debug for CxDeserializeFns<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CxDeserializeFns").finish_non_exhaustive()
    }
}

impl<M> CxDeserializeFns<M> {
    /// `deserialize`, with no entity to map.
    pub(crate) fn new(deserialize: DeserializeFn<M>) -> Self {
        Self {
            deserialize,
            cx_deserialize: default_cx_deserialize::<M>,
        }
    }
}

/// How a message or an event encodes and decodes: through `codec`, or as the stack's own
/// `ToBytes` types do.
#[derive(Debug)]
pub(crate) struct SerializeFns<M> {
    /// Called to serialize the type into the writer
    pub(crate) serialize: SerializeFn<M>,
    /// Called to deserialize the type from the reader
    pub(crate) deserialize: DeserializeFn<M>,
}

impl<M: Serialize + DeserializeOwned> Default for SerializeFns<M> {
    fn default() -> Self {
        Self {
            serialize: default_serialize::<M>,
            deserialize: default_deserialize::<M>,
        }
    }
}

/// Type of the serialize function without entity mapping
pub(crate) type SerializeFn<M> = fn(message: &M, writer: &mut Writer);

/// Type of the deserialize function without entity mapping
pub(crate) type DeserializeFn<M> = fn(reader: &mut Reader) -> Result<M, MessageError>;

/// Type of the serialize function with entity mapping.
///
/// The map is a concrete [`SendMapView`] chaining an optional shared map
/// (e.g. replicon's, on client connections) over the connection-local map.
pub(crate) type CxSerializeFn<M> =
    for<'a> fn(&'a SendMapView<'a>, message: &M, writer: &mut Writer, SerializeFn<M>);

/// Type of the deserialize function with entity mapping.
///
/// The map is a concrete [`ReceiveMapView`] chaining an optional shared map
/// (e.g. replicon's, on client connections) over the connection-local map.
pub(crate) type CxDeserializeFn<M> = for<'a> fn(
    &'a ReceiveMapView<'a>,
    reader: &mut Reader,
    DeserializeFn<M>,
) -> Result<M, MessageError>;

fn default_cx_serialize<M>(
    _: &SendMapView<'_>,
    message: &M,
    writer: &mut Writer,
    serialize_fn: SerializeFn<M>,
) {
    serialize_fn(message, writer);
}

fn default_cx_deserialize<M>(
    _: &ReceiveMapView<'_>,
    reader: &mut Reader,
    deserialize_fn: DeserializeFn<M>,
) -> Result<M, MessageError> {
    deserialize_fn(reader)
}

/// The writer as `codec`'s sink, which the orphan rule keeps from `Writer` itself.
#[derive(Debug)]
struct WriterSink<'a>(&'a mut Writer);

impl Sink for WriterSink<'_> {
    fn put(&mut self, bytes: &[u8]) {
        self.0.write_bytes(bytes);
    }
}

/// A message's encode in `codec`'s binary encoding, the one every message and component takes.
fn default_serialize<M: Serialize + ?Sized>(message: &M, writer: &mut Writer) {
    Binary::encode_to(message, &mut WriterSink(writer));
}

/// A message's decode from its one encoding in `codec`'s binary encoding; on an error, the reader
/// stays where it was.
fn default_deserialize<M: Serialize + DeserializeOwned>(
    reader: &mut Reader,
) -> Result<M, MessageError> {
    let taken = Binary::take::<M>(reader.chunk())?;
    let read = reader.remaining() - taken.rest.len();
    let value = taken.value;
    reader.advance(read);
    Ok(value)
}

impl ErasedSerializeFns {
    pub(crate) fn new<M: 'static>(
        serialize: CxSerializeFns<M>,
        deserialize: CxDeserializeFns<M>,
    ) -> Self {
        Self {
            fns: Arc::new(TypedSerializeFns {
                serialize,
                deserialize,
            }),
            maps_entities: false,
        }
    }

    /// The fns of `M`, which these must be.
    fn typed<M: 'static>(&self) -> &TypedSerializeFns<M> {
        self.fns
            .downcast_ref()
            .expect("a message's serialization fns are used as its own type")
    }

    /// Runs the context fns `serialize` and `deserialize` from now on, which map entities.
    pub(crate) fn map_entities<M: 'static>(
        &mut self,
        serialize: CxSerializeFn<M>,
        deserialize: CxDeserializeFn<M>,
    ) {
        let typed = self.typed::<M>();
        let fns = TypedSerializeFns {
            serialize: CxSerializeFns {
                serialize: typed.serialize.serialize,
                cx_serialize: serialize,
            },
            deserialize: CxDeserializeFns {
                deserialize: typed.deserialize.deserialize,
                cx_deserialize: deserialize,
            },
        };
        self.fns = Arc::new(fns);
        self.maps_entities = true;
    }

    /// Serialize the message into the writer.
    /// If available, we try to map the entities in the message from local to remote.
    pub(crate) fn serialize<M: 'static>(
        &self,
        message: &M,
        writer: &mut Writer,
        context: &SendMapView<'_>,
    ) {
        let fns = self.typed::<M>().serialize;
        (fns.cx_serialize)(context, message, writer, fns.serialize);
    }

    /// Deserialize the message value from the reader
    pub(crate) fn deserialize<M: 'static>(
        &self,
        reader: &mut Reader,
        context: &ReceiveMapView<'_>,
    ) -> Result<M, MessageError> {
        let fns = self.typed::<M>().deserialize;
        (fns.cx_deserialize)(context, reader, fns.deserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_encode_through_codec_and_decode_only_from_their_one_encoding() {
        // "ab", then 300, the varint 0xAC 0x02, as a packet carries two messages one after
        // another.
        let mut writer = Writer::with_capacity(8);
        default_serialize("ab", &mut writer);
        default_serialize(&300_u32, &mut writer);
        let written = writer.take_written();
        assert_eq!(&*written, [2, b'a', b'b', 0xAC, 0x02]);
        let mut reader = Reader::from(written);
        assert_eq!(default_deserialize::<String>(&mut reader).unwrap(), "ab");
        assert_eq!(reader.remaining(), 2);
        assert_eq!(default_deserialize::<u32>(&mut reader).unwrap(), 300);
        assert_eq!(reader.remaining(), 0);
        // 0 over-long, 0x80 0x00, is no value's one encoding: refused, and the reader stays.
        let mut reader = Reader::from(vec![0x80, 0x00]);
        assert!(matches!(
            default_deserialize::<u32>(&mut reader),
            Err(MessageError::Binary(_))
        ));
        assert_eq!(reader.remaining(), 2);
    }
}
