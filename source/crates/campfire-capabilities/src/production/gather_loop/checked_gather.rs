use bevy_ecs::entity::Entity;

use crate::production::gatherer::{GatherOrder, Load};

/// A gather order as its check leaves it: the worker, its loop, and its load.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CheckedGather {
    pub(super) entity: Entity,
    pub(super) order: Option<GatherOrder>,
    pub(super) load: Option<Load>,
}
