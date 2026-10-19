use bevy_reflect::Reflect;
use campfire_net_core::TypeKind;
use core::any::TypeId;

/// [`ComponentKind`] is an internal wrapper around the type of the component
#[derive(Debug, Eq, Hash, Copy, Clone, PartialEq, PartialOrd, Ord, Reflect)]
pub struct ComponentKind(pub TypeId);

impl ComponentKind {
    pub const fn of<C: 'static>() -> Self {
        Self(TypeId::of::<C>())
    }
}

impl TypeKind for ComponentKind {}

impl From<TypeId> for ComponentKind {
    fn from(type_id: TypeId) -> Self {
        Self(type_id)
    }
}
