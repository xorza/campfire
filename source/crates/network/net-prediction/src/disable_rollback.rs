use core::fmt::Debug;

use bevy_ecs::prelude::*;

/// An entity that no rollback checks or returns to a past state.
#[derive(Component, Debug, Default)]
pub struct DisableRollback;
