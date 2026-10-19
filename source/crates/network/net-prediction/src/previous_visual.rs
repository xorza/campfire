use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_interpolation::FrameInterpolate;
use core::fmt::Debug;

/// The value a corrected component showed before a rollback or a timeline switch moved it,
/// which a visual correction is measured from; `None` while no jump waits to be measured.
///
/// The slot goes on with the component's prediction history and stays: a rollback writes it in
/// place, so a correction moves no entity between tables. A correction is measured against the
/// frame-interpolated value, so the slot brings frame interpolation with it.
#[derive(Component, Debug, Reflect)]
#[require(FrameInterpolate)]
pub struct PreviousVisual<C: Component>(pub Option<C>);

impl<C: Component> Default for PreviousVisual<C> {
    fn default() -> Self {
        Self(None)
    }
}
