use std::marker::PhantomData;

use bevy_ecs::world::World;
use bevy_replicon::prelude::{ComponentsScope, FilterScope};
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_replicon::shared::replication::visibility::VisibilityScope;

/// The components of the scopes `A` and `B` as one scope, which a filter hides together: a kind
/// of more types than replicon's tuples take, 10, is a union of two tuples.
#[derive(Debug)]
pub(crate) struct ScopeUnion<A, B>(PhantomData<(A, B)>);

impl<A: ComponentsScope, B: ComponentsScope> FilterScope for ScopeUnion<A, B> {
    fn visibility_scope(world: &mut World, registry: &mut ReplicationRegistry) -> VisibilityScope {
        let (VisibilityScope::Components(mut mask), VisibilityScope::Components(other)) = (
            A::visibility_scope(world, registry),
            B::visibility_scope(world, registry),
        ) else {
            unreachable!("a components scope yields components");
        };
        mask |= &other;
        VisibilityScope::Components(mask)
    }
}

impl<A: ComponentsScope, B: ComponentsScope> ComponentsScope for ScopeUnion<A, B> {}
