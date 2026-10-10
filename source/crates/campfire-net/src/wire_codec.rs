use bevy_ecs::component::Component;
use bevy_ecs::error::Result as SystemResult;
use bevy_replicon::bytes::Bytes;
use bevy_replicon::prelude::RuleFns;
use bevy_replicon::shared::replication::registry::ctx::{SerializeCtx, WriteCtx};
use campfire_common::{Binary, BinaryError, Sink};
use lightyear_serde::SerializationError;
use lightyear_serde::reader::Reader;
use lightyear_serde::registry::SerializeFns;
use lightyear_serde::writer::Writer;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The encode and decode Lightyear's messages and replicated components register with: the
/// gateway's (design 02, Serialization), so what crosses the network decodes from its one
/// encoding, as every other postcard value does.
#[derive(Debug)]
pub(crate) struct WireCodec;

/// A Lightyear writer as a sink of the gateway's batches.
#[derive(Debug)]
struct WriterSink<'a>(&'a mut Writer);

impl Sink for WriterSink<'_> {
    fn put(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

impl WireCodec {
    /// A message's encode and decode.
    pub(crate) fn message<M: Serialize + DeserializeOwned>() -> SerializeFns<M> {
        SerializeFns {
            serialize: WireCodec::write_message,
            deserialize: WireCodec::read_message,
        }
    }

    /// A replicated component's encode and decode.
    pub(crate) fn component<C: Component + Serialize + DeserializeOwned>() -> RuleFns<C> {
        RuleFns::new(WireCodec::write_component, WireCodec::read_component)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "Lightyear's function type returns a Result"
    )]
    fn write_message<M: Serialize>(
        message: &M,
        writer: &mut Writer,
    ) -> Result<(), SerializationError> {
        Binary::encode_to(message, &mut WriterSink(writer));
        Ok(())
    }

    /// The message at the front of what `reader` has left, which it then passes.
    fn read_message<M: Serialize + DeserializeOwned>(
        reader: &mut Reader,
    ) -> Result<M, SerializationError> {
        let left = reader.remaining_slice();
        let taken = Binary::take::<M>(left).map_err(|error| match error {
            BinaryError::Malformed(error) => SerializationError::Postcard(error),
            BinaryError::Truncated | BinaryError::NotCanonical => SerializationError::InvalidValue,
        })?;
        let read =
            u64::try_from(left.len() - taken.rest.len()).expect("a message's length fits u64");
        let value = taken.value;
        reader.set_position(reader.position() + read);
        Ok(value)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "replicon's function type returns a Result"
    )]
    fn write_component<C: Serialize>(
        _: &mut SerializeCtx<'_>,
        component: &C,
        out: &mut Vec<u8>,
    ) -> SystemResult<()> {
        Binary::encode_into(component, out);
        Ok(())
    }

    /// The component at the front of `bytes`, which then start past it.
    fn read_component<C: Serialize + DeserializeOwned>(
        _: &mut WriteCtx<'_>,
        bytes: &mut Bytes,
    ) -> SystemResult<C> {
        let taken = Binary::take::<C>(bytes)?;
        let read = bytes.len() - taken.rest.len();
        let value = taken.value;
        *bytes = bytes.slice(read..);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_read_back_in_order_and_only_from_their_one_encoding() {
        // Two messages in one buffer, as a packet carries them: "ab", then 300, the varint
        // 0xAC 0x02. Each read takes its own bytes, and the reader stops past them.
        let mut writer = Writer::with_capacity(8);
        WireCodec::write_message(&"ab".to_owned(), &mut writer).unwrap();
        WireCodec::write_message(&300_u32, &mut writer).unwrap();
        let written = writer.take_written();
        assert_eq!(&written[..], [2, b'a', b'b', 0xAC, 0x02]);
        let mut reader = Reader::from(written);
        assert_eq!(
            WireCodec::read_message::<String>(&mut reader).unwrap(),
            "ab"
        );
        assert_eq!(reader.position(), 3);
        assert_eq!(WireCodec::read_message::<u32>(&mut reader).unwrap(), 300);
        assert!(reader.remaining_slice().is_empty());
        // 0 over-long, 0x80 0x00, is refused, and the reader does not move.
        let mut reader = Reader::from(vec![0x80, 0x00]);
        assert!(matches!(
            WireCodec::read_message::<u32>(&mut reader),
            Err(SerializationError::InvalidValue)
        ));
        assert_eq!(reader.position(), 0);
    }
}
