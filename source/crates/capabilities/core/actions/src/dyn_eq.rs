use std::any::Any;

/// Equality of two members of an open set, whatever their types: the same type, and equal.
pub trait DynEq {
    fn eq_dyn(&self, other: &dyn Any) -> bool;
}

impl<T: Eq + 'static> DynEq for T {
    fn eq_dyn(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}
