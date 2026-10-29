use core::fmt::Debug;

use bevy_ecs::prelude::*;

#[derive(Debug, Component)]
/// Marker `Disabled` component inserted on `DisableRollback` entities during rollbacks so
/// that they are ignored from all queries
pub struct DisabledDuringRollback;
