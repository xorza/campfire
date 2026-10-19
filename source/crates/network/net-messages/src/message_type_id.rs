use bevy_reflect::Reflect;
use campfire_net_core::RegisteredType;
use core::any::TypeId;
use core::hash::Hash;

/// [`MessageTypeId`] is an internal wrapper around the type of the message
#[derive(Debug, Eq, Hash, Copy, Clone, PartialEq, Reflect)]
pub struct MessageTypeId(TypeId);

impl MessageTypeId {
    #[inline(always)]
    pub const fn of<M: 'static>() -> Self {
        Self(TypeId::of::<M>())
    }
}

impl RegisteredType for MessageTypeId {}

impl From<TypeId> for MessageTypeId {
    fn from(type_id: TypeId) -> Self {
        Self(type_id)
    }
}
