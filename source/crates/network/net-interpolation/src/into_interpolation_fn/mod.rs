//! The constructors of a rule's interpolation fn, one for each shape of fn, apart by a marker type
//! as Bevy's `IntoSystem<In, Out, Marker>` keeps its fn shapes apart.
use bevy_ecs::component::Component;
use campfire_net_frame_interpolation::InterpolationSampleCx;

use crate::interpolation_fn::{ComponentInterpolation, InterpolationFn};

/// A fn of a fraction that interpolates the rule's target `S`: `Fn(&C, &C, f32) -> C` for a
/// component, `Fn((&A, &B), (&A, &B), f32) -> (A, B)` for a bundle (11-net-stack/api.md, A rule's
/// fn goes through one generic constructor for each shape).
///
/// `M` tells the shapes apart, so a fn item, a fn pointer or a closure with typed parameters
/// infers it; a closure whose parameters are untyped needs their types.
pub trait IntoLerpFn<S, M>: Send + Sync + 'static {
    fn into_interpolation_fn(self) -> InterpolationFn<S>;
}

/// A fn of the sample's context that interpolates the rule's target `S`:
/// `Fn(&C, &C, InterpolationSampleCx) -> C` for a component, with a tuple of references for
/// a bundle, as [`IntoLerpFn`] takes them.
pub trait IntoCxLerpFn<S, M>: Send + Sync + 'static {
    fn into_interpolation_fn(self) -> InterpolationFn<S>;
}

/// The marker of a component's fn shape.
#[derive(Debug)]
pub struct ComponentShape;

impl<C: Component, F> IntoLerpFn<C, ComponentShape> for F
where
    F: Fn(&C, &C, f32) -> C + Send + Sync + 'static,
{
    fn into_interpolation_fn(self) -> InterpolationFn<C> {
        let interpolation: Box<ComponentInterpolation<C>> =
            Box::new(move |start: &C, other: &C, cx: InterpolationSampleCx| {
                self(start, other, cx.t)
            });
        InterpolationFn::erase(interpolation)
    }
}

impl<C: Component, F> IntoCxLerpFn<C, ComponentShape> for F
where
    F: Fn(&C, &C, InterpolationSampleCx) -> C + Send + Sync + 'static,
{
    fn into_interpolation_fn(self) -> InterpolationFn<C> {
        let interpolation: Box<ComponentInterpolation<C>> = Box::new(self);
        InterpolationFn::erase(interpolation)
    }
}

#[cfg(test)]
mod tests;
