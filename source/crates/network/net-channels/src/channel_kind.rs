use crate::channel::Channel;
use campfire_net_core::TypeKind;
use core::any::TypeId;

/// The type of a registered [`Channel`], which names the channel within an app.
#[derive(Debug, Eq, Hash, Copy, Clone, PartialEq)]
pub struct ChannelKind(pub TypeId);

impl ChannelKind {
    pub const fn of<C: Channel>() -> Self {
        Self(TypeId::of::<C>())
    }
}

impl TypeKind for ChannelKind {}

impl From<TypeId> for ChannelKind {
    fn from(type_id: TypeId) -> Self {
        Self(type_id)
    }
}
