use crate::channel::Channel;
use campfire_net_core::RegisteredType;
use core::any::TypeId;

/// The type of a registered [`Channel`], which names the channel within an app.
#[derive(Debug, Eq, Hash, Copy, Clone, PartialEq)]
pub struct ChannelTypeId(pub TypeId);

impl ChannelTypeId {
    pub const fn of<C: Channel>() -> Self {
        Self(TypeId::of::<C>())
    }
}

impl RegisteredType for ChannelTypeId {}

impl From<TypeId> for ChannelTypeId {
    fn from(type_id: TypeId) -> Self {
        Self(type_id)
    }
}
