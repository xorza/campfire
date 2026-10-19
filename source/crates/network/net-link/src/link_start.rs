use bevy_ecs::prelude::*;

/// Entity event requesting that a transport start establishing a [`Link`](crate::link::Link).
///
/// `LinkStart` is transport-facing: A transport plugin observes this event for its own link
/// entities and inserts [`Linking`](crate::link_state::Linking) or
/// [`Linked`](crate::link_state::Linked) when the connection progresses.
#[derive(Debug, EntityEvent)]
pub struct LinkStart {
    /// Entity that owns the [`Link`](crate::link::Link) to start.
    pub entity: Entity,
}
