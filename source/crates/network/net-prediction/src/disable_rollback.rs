use bevy_ecs::prelude::*;
use core::fmt::Debug;

/// Marker component to indicate that the entity will be completely excluded from rollbacks. It
/// won't be part of rollback checks, and it won't be rolled back to a past state if a rollback
/// happens.
#[derive(Component, Debug, Default)]
pub struct DisableRollback;
