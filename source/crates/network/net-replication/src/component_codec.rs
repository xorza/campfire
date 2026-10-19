use bevy_ecs::component::Component;
use bevy_ecs::error::Result;
use bevy_replicon::prelude::RuleFns;
use bevy_replicon::shared::replication::registry::ctx::{SerializeCtx, WriteCtx};
use bevy_replicon::shared::replication::registry::receive_fns::MutWrite;
use bytes::Bytes;
use campfire_codec::{Binary, BinaryError};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// How a replicated component, and every value the replication writes into Replicon's messages,
/// encodes: in `codec`'s binary encoding, and decodes only from its one encoding.
#[derive(Debug)]
pub(crate) struct ComponentCodec;

impl ComponentCodec {
    /// The rule functions of `C`, which encode and decode it through `codec`.
    pub(crate) fn rule_fns<C>() -> RuleFns<C>
    where
        C: Component<Mutability: MutWrite<C>> + Serialize + DeserializeOwned,
    {
        RuleFns::new(Self::write_component::<C>, Self::read_component::<C>)
    }

    /// Appends `value` to `message`.
    pub(crate) fn write<T: Serialize + ?Sized>(value: &T, message: &mut Vec<u8>) {
        Binary::encode_into(value, message);
    }

    /// The value at the front of `message`, which then starts past it.
    pub(crate) fn read<T: Serialize + DeserializeOwned>(
        message: &mut Bytes,
    ) -> Result<T, BinaryError> {
        let taken = Binary::take::<T>(message)?;
        let read = message.len() - taken.rest.len();
        let value = taken.value;
        *message = message.slice(read..);
        Ok(value)
    }

    /// A value at the front of `message` that Replicon itself encoded, which then starts past
    /// it: a type Replicon gives no `Serialize`, as its diffs, so no check that the bytes are
    /// its one encoding.
    pub(crate) fn read_replicon<T: DeserializeOwned>(
        message: &mut Bytes,
    ) -> Result<T, BinaryError> {
        let taken = Binary::take_unchecked::<T>(message)?;
        let read = message.len() - taken.rest.len();
        let value = taken.value;
        *message = message.slice(read..);
        Ok(value)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "Replicon's serialize fn returns a Result"
    )]
    fn write_component<C: Serialize>(
        _: &mut SerializeCtx<'_>,
        component: &C,
        message: &mut Vec<u8>,
    ) -> Result<()> {
        Self::write(component, message);
        Ok(())
    }

    /// The component at the front of `message`, the server's entities it holds mapped to the
    /// client's, as Replicon's own default maps them.
    fn read_component<C: Component + Serialize + DeserializeOwned>(
        ctx: &mut WriteCtx<'_>,
        message: &mut Bytes,
    ) -> Result<C> {
        let mut component: C = Self::read(message)?;
        C::map_entities(&mut component, ctx);
        Ok(component)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_reads_back_from_its_one_encoding_and_the_message_moves_past_it() {
        // 300, the varint 0xAC 0x02, then 7.
        let mut written = Vec::new();
        ComponentCodec::write(&300_u32, &mut written);
        ComponentCodec::write(&7_u8, &mut written);
        assert_eq!(written, [0xAC, 0x02, 7]);
        let mut message = Bytes::from(written);
        assert_eq!(ComponentCodec::read::<u32>(&mut message).unwrap(), 300);
        assert_eq!(&*message, [7]);
        // 0 over-long is refused, and the message stays.
        let mut message = Bytes::from_static(&[0x80, 0x00]);
        assert!(ComponentCodec::read::<u32>(&mut message).is_err());
        assert_eq!(message.len(), 2);
    }
}
