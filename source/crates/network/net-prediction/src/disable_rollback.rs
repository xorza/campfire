use bevy_ecs::prelude::*;
use core::fmt::Debug;

/// An entity that no rollback checks or returns to a past state.
#[derive(Component, Debug, Default)]
pub struct DisableRollback;
