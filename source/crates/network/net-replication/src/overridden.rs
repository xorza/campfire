use bevy_ecs::prelude::*;
use core::any::type_name;
use core::fmt;
use core::marker::PhantomData;

/// Marks a hierarchy member that owns its `A`: the member no longer follows the `A` of its
/// [`ReplicateLike`](crate::replicate_like::ReplicateLike) root.
///
/// `A` is [`Replicate`](crate::replication_target::Replicate),
/// [`PredictionTarget`](crate::replication_target::PredictionTarget),
/// [`InterpolationTarget`](crate::replication_target::InterpolationTarget),
/// [`Rooms`](crate::rooms::Rooms) or
/// `ManualVisibility`. The hierarchy inserts it when
/// an entity joins with an `A` that differs from its root's, and when a user write leaves a
/// member's `A` different from its root's. A write equal to the root's removes it. Remove it to
/// follow the root again.
#[derive(Component)]
pub struct Overridden<A: 'static>(PhantomData<fn() -> A>);

impl<A> Default for Overridden<A> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<A> Clone for Overridden<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A> Copy for Overridden<A> {}

impl<A> fmt::Debug for Overridden<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Overridden<{}>", type_name::<A>())
    }
}
