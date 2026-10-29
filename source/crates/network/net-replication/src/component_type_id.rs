use core::any::TypeId;

use bevy_reflect::Reflect;
use campfire_net_core::RegisteredType;

/// [`ComponentTypeId`] is an internal wrapper around the type of the component
#[derive(Debug, Eq, Hash, Copy, Clone, PartialEq, PartialOrd, Ord, Reflect)]
pub struct ComponentTypeId(pub TypeId);

impl ComponentTypeId {
    pub const fn of<C: 'static>() -> Self {
        Self(TypeId::of::<C>())
    }
}

impl RegisteredType for ComponentTypeId {}

impl From<TypeId> for ComponentTypeId {
    fn from(type_id: TypeId) -> Self {
        Self(type_id)
    }
}
