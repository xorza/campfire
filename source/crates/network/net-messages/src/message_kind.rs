use bevy_reflect::Reflect;
use campfire_net_core::TypeKind;
use core::any::TypeId;
use core::hash::Hash;

/// [`MessageKind`] is an internal wrapper around the type of the message
#[derive(Debug, Eq, Hash, Copy, Clone, PartialEq, Reflect)]
pub struct MessageKind(TypeId);

impl MessageKind {
    #[inline(always)]
    pub const fn of<M: 'static>() -> Self {
        Self(TypeId::of::<M>())
    }
}

impl TypeKind for MessageKind {}

impl From<TypeId> for MessageKind {
    fn from(type_id: TypeId) -> Self {
        Self(type_id)
    }
}
